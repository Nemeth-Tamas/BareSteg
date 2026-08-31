mod bmp;
mod carrier;
mod crc32;
mod ecc;
mod frame;

use std::{env, fs, process};

use bmp::Bmp;

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
    let protected_header_len = ecc::encoded_header_len(frame::HEADER_LEN)?;
    let protected_header = carrier::extract_bytes(&image, protected_header_len)?;
    let (header, header_stats) =
        ecc::decode_header_with_stats(&protected_header, frame::HEADER_LEN)?;

    println!(
        "Header ECC votes: {}/{} protected copies disagreed with majority across {}/{} logical bits",
        header_stats.minority_votes,
        header_stats.protected_votes,
        header_stats.disputed_bits,
        header_stats.logical_bits
    );

    let payload_len = frame::payload_len_from_header(&header)?;
    let protected_frame_len = ecc::encoded_frame_len(frame::HEADER_LEN, payload_len)?;
    let protected_frame = carrier::extract_bytes(&image, protected_frame_len)?;
    let (recovered_frame, recovery_stats) =
        ecc::decode_frame_with_stats(&protected_frame, frame::HEADER_LEN, payload_len)?;
    let payload = frame::decode(&recovered_frame)?;

    println!(
        "ECC votes: {}/{} protected copies disagreed with majority across {}/{} logical bits",
        recovery_stats.minority_votes,
        recovery_stats.protected_votes,
        recovery_stats.disputed_bits,
        recovery_stats.logical_bits
    );

    fs::write(output_path, &payload)
        .map_err(|error| format!("failed to write recovered payload '{output_path}': {error}"))?;

    println!(
        "Recovered {} payload bytes from {} -> {} (CRC32 OK)",
        payload.len(),
        image_path,
        output_path
    );

    Ok(())
}

fn usage() -> String {
    [
        "usage:",
        "  baresteg hide <carrier.bmp> <payload> <output.bmp>",
        "  baresteg reveal <image.bmp> <output>",
    ]
    .join("\n")
}
