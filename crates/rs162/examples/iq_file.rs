use deku::DekuReader;
use rs162::decode::ais::Message;
use rs162::dsp::ais::{AisDemodulatedMessage, AisDemodulator, AIS_SAMPLE_RATE_96K};
use rs162::dsp::{
    convert_samples_cf32, convert_samples_cs16, convert_samples_cs8, convert_samples_cu8,
};
use serde_json::json;
use std::env;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

const CHUNK_SIZE: usize = 16384 * 2;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <iq_file> [format]", args[0]);
        eprintln!("  iq_file: Path to IQ samples file (must be 96kHz sample rate)");
        eprintln!("  format: Sample format (default: cu8)");
        eprintln!("    cu8   - Complex unsigned 8-bit (RTLSDR format)");
        eprintln!("    cs8   - Complex signed 8-bit");
        eprintln!("    cs16  - Complex signed 16-bit");
        eprintln!("    cf32  - Complex 32-bit float");
        eprintln!();
        eprintln!("Note: This demodulator expects 96kHz sample rate.");
        eprintln!("Use SoX to resample your file first:");
        eprintln!("sox -t raw -r 1600000 -b 8 -c 2 -e unsigned-integer input.bin \\");
        eprintln!("    -t raw -r 96000 -b 8 -c 2 -e unsigned-integer output.bin");
        std::process::exit(1);
    }

    let filename = &args[1];

    let format = if args.len() > 2 {
        args[2].as_str()
    } else {
        "cu8"
    };

    match format {
        "cu8" | "cs8" | "cs16" | "cf32" => {}
        _ => {
            eprintln!(
                "Error: Unsupported format '{}'. Use cu8, cs8, cs16, or cf32",
                format
            );
            std::process::exit(1);
        }
    }

    let sample_rate = if args[3].ends_with('k') {
        let num = args[3][..args[3].len() - 1]
            .parse::<u32>()
            .unwrap_or(AIS_SAMPLE_RATE_96K);
        num * 1000
    } else {
        args[3].parse::<u32>().unwrap_or(AIS_SAMPLE_RATE_96K)
    };

    eprintln!("Reading IQ samples from: {}", filename);
    eprintln!(
        "Sample rate: {} Hz ({} kHz)",
        sample_rate,
        sample_rate / 1000
    );
    eprintln!("Format: {}", format);

    if !Path::new(filename).exists() {
        eprintln!("Error: File '{}' does not exist", filename);
        std::process::exit(1);
    }

    let file = File::open(filename)?;
    let mut reader = BufReader::new(file);

    let mut demodulator = AisDemodulator::new(sample_rate);
    let mut total_messages = 0;
    let mut total_chunks = 0;

    match format {
        "cu8" => process_cu8(
            &mut reader,
            &mut demodulator,
            &mut total_chunks,
            &mut total_messages,
        )?,
        "cs8" => process_cs8(
            &mut reader,
            &mut demodulator,
            &mut total_chunks,
            &mut total_messages,
        )?,
        "cs16" => process_cs16(
            &mut reader,
            &mut demodulator,
            &mut total_chunks,
            &mut total_messages,
        )?,
        "cf32" => process_cf32(
            &mut reader,
            &mut demodulator,
            &mut total_chunks,
            &mut total_messages,
        )?,
        _ => unreachable!(),
    }

    Ok(())
}

fn process_cu8(
    reader: &mut BufReader<File>,
    demodulator: &mut AisDemodulator,
    total_chunks: &mut usize,
    total_messages: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![0u8; CHUNK_SIZE];

    loop {
        match reader.read(&mut buffer)? {
            0 => break,
            bytes_read => {
                *total_chunks += 1;
                let samples_to_process = (bytes_read / 2) * 2;
                if samples_to_process == 0 {
                    continue;
                }

                let iq_samples = convert_samples_cu8(&buffer);
                let messages = demodulator.demodulate(&iq_samples);
                for demod_msg in messages {
                    if let Ok(Some(ais_msg)) = process_demodulated_message(&demod_msg) {
                        let output = json!({
                            "signal_level": demod_msg.signal_level,
                            "timestamp": demod_msg.timestamp,
                            "channel": demod_msg.channel,
                            "message": ais_msg
                        });
                        println!("{}", serde_json::to_string(&output)?);
                        *total_messages += 1;
                    }
                }
            }
        }
    }
    Ok(())
}

fn process_cs8(
    reader: &mut BufReader<File>,
    demodulator: &mut AisDemodulator,
    total_chunks: &mut usize,
    total_messages: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![0i8; CHUNK_SIZE];
    let mut byte_buffer = vec![0u8; CHUNK_SIZE];

    loop {
        match reader.read(&mut byte_buffer)? {
            0 => break,
            bytes_read => {
                *total_chunks += 1;
                let samples_to_process = (bytes_read / 2) * 2;
                if samples_to_process == 0 {
                    continue;
                }

                for i in 0..samples_to_process {
                    buffer[i] = byte_buffer[i] as i8;
                }

                let iq_samples = convert_samples_cs8(&buffer[..samples_to_process]);
                let messages = demodulator.demodulate(&iq_samples);

                for demod_msg in messages {
                    if let Ok(Some(ais_msg)) = process_demodulated_message(&demod_msg) {
                        let output = json!({
                            "signal_level": demod_msg.signal_level,
                            "timestamp": demod_msg.timestamp,
                            "channel": demod_msg.channel,
                            "message": ais_msg
                        });
                        println!("{}", serde_json::to_string(&output)?);
                        *total_messages += 1;
                    }
                }
            }
        }
    }
    Ok(())
}

fn process_cs16(
    reader: &mut BufReader<File>,
    demodulator: &mut AisDemodulator,
    total_chunks: &mut usize,
    total_messages: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![0i16; CHUNK_SIZE];
    let mut byte_buffer = vec![0u8; CHUNK_SIZE * 2];

    loop {
        match reader.read(&mut byte_buffer)? {
            0 => break,
            bytes_read => {
                *total_chunks += 1;
                let samples_to_process = (bytes_read / 4) * 2;
                if samples_to_process == 0 {
                    continue;
                }

                for i in 0..samples_to_process {
                    let bytes = [byte_buffer[i * 2], byte_buffer[i * 2 + 1]];
                    buffer[i] = i16::from_le_bytes(bytes);
                }

                let iq_samples = convert_samples_cs16(&buffer[..samples_to_process]);
                let messages = demodulator.demodulate(&iq_samples);

                for demod_msg in messages {
                    if let Ok(Some(ais_msg)) = process_demodulated_message(&demod_msg) {
                        let output = json!({
                            "signal_level": demod_msg.signal_level,
                            "timestamp": demod_msg.timestamp,
                            "channel": demod_msg.channel,
                            "message": ais_msg
                        });
                        println!("{}", serde_json::to_string(&output)?);
                        *total_messages += 1;
                    }
                }
            }
        }
    }
    Ok(())
}

fn process_cf32(
    reader: &mut BufReader<File>,
    demodulator: &mut AisDemodulator,
    total_chunks: &mut usize,
    total_messages: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = vec![0f32; CHUNK_SIZE];
    let mut byte_buffer = vec![0u8; CHUNK_SIZE * 4];

    loop {
        match reader.read(&mut byte_buffer)? {
            0 => break,
            bytes_read => {
                *total_chunks += 1;
                let floats_to_process = (bytes_read / 4) & !1;
                if floats_to_process == 0 {
                    continue;
                }

                for i in 0..floats_to_process {
                    let bytes = [
                        byte_buffer[i * 4],
                        byte_buffer[i * 4 + 1],
                        byte_buffer[i * 4 + 2],
                        byte_buffer[i * 4 + 3],
                    ];
                    buffer[i] = f32::from_le_bytes(bytes);
                }

                let iq_samples = convert_samples_cf32(&buffer[..floats_to_process]);
                let messages = demodulator.demodulate(&iq_samples);

                for demod_msg in messages {
                    if let Ok(Some(ais_msg)) = process_demodulated_message(&demod_msg) {
                        let output = json!({
                            "signal_level": demod_msg.signal_level,
                            "timestamp": demod_msg.timestamp,
                            "channel": demod_msg.channel,
                            "message": ais_msg
                        });
                        println!("{}", serde_json::to_string(&output)?);
                        *total_messages += 1;
                    }
                }
            }
        }
    }
    Ok(())
}

fn process_demodulated_message(
    demod_msg: &AisDemodulatedMessage,
) -> Result<Option<Message>, Box<dyn std::error::Error>> {
    if demod_msg.bits.len() < 14 {
        return Ok(None);
    }

    let cursor = std::io::Cursor::new(&demod_msg.bits);
    let mut reader = deku::prelude::Reader::new(cursor);

    match Message::from_reader_with_ctx(&mut reader, ()) {
        Ok(message) => Ok(Some(message)),
        Err(_) => Ok(None),
    }
}
