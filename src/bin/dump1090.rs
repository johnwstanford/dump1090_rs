
#[macro_use]
extern crate lazy_static;

lazy_static! {
	static ref MODES:Mutex<dump1090_rs::Modes> = Mutex::new({
		dump1090_rs::Modes::default()
	});
}

use std::io::Cursor;
use std::io::prelude::*;
use std::fs::File;
use std::sync::Mutex;

use byteorder::{BigEndian, ReadBytesExt};
use clap::{Arg, App};

use dump1090_rs::{rtlsdr, MagnitudeBuffer, MODES_MAG_BUF_SAMPLES};

fn main() -> Result<(), &'static str> {

	let matches = App::new("Rust dump1090")
		.version("0.1.0")
		.author("John Stanford (johnwstanford@gmail.com)")
		.about("Translation of dump1090-mutability into Rust, intended to match bit-for-bit")
		.arg(Arg::with_name("ifile").long("ifile")
			.help("Read data from file")
			.required(false).takes_value(true))
		.arg(Arg::with_name("throttle").long("throttle")
			.help("When reading from a file, play back in realtime, not at max speed")
			.required(false).takes_value(false))
		.get_matches();

	let mut f_buffer:[u8; 2*MODES_MAG_BUF_SAMPLES] = [0u8; 2*MODES_MAG_BUF_SAMPLES];
	let mut active:bool = true;

	let fs:usize = 2400000;

	let mut src:Box<dyn std::io::Read> = if let Some(fname) = matches.value_of("ifile") {
		
		let f = File::open(fname).unwrap();
		Box::new(f)

	} else {

		let mut dev = rtlsdr::RtlSdrDevice::new(0)?;

		let available_gains = dev.get_tuner_gains()?;
		eprintln!("Available gains: {:?}", available_gains);

		let max_gain:i32 = *(available_gains.iter().max().unwrap());
		eprintln!("Max available gain: {:.1} [dB]", (max_gain as f32) * 0.1);

		dev.set_tuner_gain_mode(1)?;
		dev.set_tuner_gain(max_gain)?;
		if let Err(_) = dev.set_freq_correction(0) {
			// For some reason, this function returns -2 when we set the frequency correction to 0
			// The same thing happens in dump1090, but the return value is never checked
			eprintln!("Warning: Nonzero return value from set_freq_correction");
		}
		dev.set_center_freq(1090_000_000)?;
		dev.set_sample_rate(2400000)?;

		eprintln!("Set center freq to {:.4e} [Hz]", dev.get_center_freq()?);
		eprintln!("Set freq correction to {} [ppm]", dev.get_freq_correction()?);
		eprintln!("Set tuner gain to {:.1} [dB]", (dev.get_tuner_gain()? as f32) * 0.1);
		eprintln!("Set sample rate to {}", dev.get_sample_rate()?);

		dev.reset_buffer()?;
		Box::new(dev)

	};

	// TODO: Use the throttle argument

	while active {

		if let Ok(mut modes) = MODES.lock() {

			let outbuf:&mut MagnitudeBuffer = modes.next_buffer(fs);

			let mut total_power_u64:u64 = 0;

			let read_result = src.read(&mut f_buffer);
			match read_result {
				Err(_) => active = false,
				Ok(0)  => active = false,
				Ok(n)  => {
					let mut rdr = Cursor::new(&f_buffer[..n]);

					// The choice of BigEndian vs LittleEndian determines which is I and which is Q but
					// since we're just taking the magnitude, it doesn't matter
					while let Ok(iq) = rdr.read_u16::<BigEndian>() {

						let this_mag:u16 = dump1090_rs::MAG_LUT[iq as usize];

						outbuf.push(this_mag);
						
						total_power_u64 += this_mag as u64 * this_mag as u64;    

					}

				}
			}

			outbuf.total_power = (total_power_u64 as f64) / 65535.0 / 65535.0;

			dump1090_rs::demod_2400::demodulate2400(&outbuf, fs).unwrap();

		}

	}

	Ok(())

}