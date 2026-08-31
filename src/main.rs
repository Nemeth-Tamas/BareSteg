mod bmp;
mod carrier;
mod crc32;
mod ecc;
mod frame;

use std::{env, fs, process};

use bmp::Bmp;

const DECODE_QUANTIZATION_STEPS: [i32; 12] = [8, 7, 6, 5, 4, 3, 2, 1, 9, 10, 11, 12];

fn main() {
    if let Err(error) = run() {
        eprintln!("BareSteg error: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let arguments: Vec<String> = env::args().collect();

    match arguments.as_slice() {
        [_, command, carrier_path, payload_path, output_path] if command == "hide" => {
            hide(carrier_path, payload_path, output_path)
        }
        [_, command, image_path, output_path] if command == "reveal" => {
            reveal(image_path, output_path)
        }
        _ => Err(usage()),
    }
}

fn hide(carrier_path: &str, payload_path: &str, output_path: &str) -> Result<(), String> {
    let payload = fs::read(payload_path)
        .map_err(|error| format!("failed to read payload '{payload_path}': {error}"))?;
    let frame = frame::encode(&payload)?;
    let protected_frame = ecc::encode_frame(&frame, frame::HEADER_LEN)?;
    let mut image = Bmp::load(carrier_path)?;

    carrier::embed(&mut image, &protected_frame)?;
    image.save(output_path)?;

    println!(
        "Hidden {} payload bytes in {}x{} BMP -> {}",
        payload.len(),
        image.width(),
        image.height(),
        output_path
    );

    Ok(())
}

fn reveal(image_path: &str, output_path: &str) -> Result<(), String> {
    let image = Bmp::load(image_path)?;
    let mut last_error = None;

    for quantization_step in DECODE_QUANTIZATION_STEPS {
        match recover_with_quantization_step(&image, quantization_step) {
            Ok((payload, header_stats, recovery_stats)) => {
                println!("QIM decode step: {quantization_step}");

                println!(
                    "Header ECC votes: {}/{} protected copies disagreed with majority across {}/{} logical bits",
                    header_stats.minority_votes,
                    header_stats.protected_votes,
                    header_stats.disputed_bits,
                    header_stats.logical_bits
                );

                println!(
                    "ECC votes: {}/{} protected copies disagreed with majority across {}/{} logical bits",
                    recovery_stats.minority_votes,
                    recovery_stats.protected_votes,
                    recovery_stats.disputed_bits,
                    recovery_stats.logical_bits
                );

                fs::write(output_path, &payload).map_err(|error| {
                    format!("failed to write recovered payload '{output_path}': {error}")
                })?;

                println!(
                    "Recovered {} payload bytes from {} -> {} (CRC32 OK)",
                    payload.len(),
                    image_path,
                    output_path
                );

                return Ok(());
            }
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    Err(format!(
        "failed to recover BareSteg frame with QIM steps 1 through 12; last error: {}",
        last_error.unwrap_or_else(|| "no decoder candidate was attempted".to_string())
    ))
}

fn recover_with_quantization_step(
    image: &Bmp,
    quantization_step: i32,
) -> Result<(Vec<u8>, ecc::DecodeStats, ecc::DecodeStats), String> {
    let protected_header_len = ecc::encoded_header_len(frame::HEADER_LEN)?;
    let protected_header = carrier::extract_bytes(image, protected_header_len, quantization_step)?;

    let (header, header_stats) =
        ecc::decode_header_with_stats(&protected_header, frame::HEADER_LEN)?;

    let payload_len = frame::payload_len_from_header(&header)?;
    let protected_frame_len = ecc::encoded_frame_len(frame::HEADER_LEN, payload_len)?;

    let protected_frame = carrier::extract_bytes(image, protected_frame_len, quantization_step)?;

    let (recovered_frame, recovery_stats) =
        ecc::decode_frame_with_stats(&protected_frame, frame::HEADER_LEN, payload_len)?;

    let payload = frame::decode(&recovered_frame)?;

    Ok((payload, header_stats, recovery_stats))
}

fn usage() -> String {
    [
        "usage:",
        "  baresteg hide <carrier.bmp> <payload> <output.bmp>",
        "  baresteg reveal <image.bmp> <output>",
    ]
    .join("\n")
}
