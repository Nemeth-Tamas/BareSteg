mod bmp;
mod carrier;
mod crc32;
mod ecc;
mod frame;

use std::{env, fs, process};

use bmp::Bmp;

const DECODE_QUANTIZATION_STEPS: [i32; 12] = [8, 9, 7, 10, 6, 11, 5, 12, 4, 3, 2, 1];
const MAX_HEADER_IDENTITY_REPAIRS: usize = 8;
const MAX_PAYLOAD_LENGTH_REPAIRS: usize = 4;
const MAX_PAYLOAD_CRC_REPAIRS: usize = 4;

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
            Ok((payload, header_stats, recovery_stats, header_repairs)) => {
                println!("QIM decode step: {quantization_step}");
                println!("Header repairs: {header_repairs} bit(s)");

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
) -> Result<(Vec<u8>, ecc::DecodeStats, ecc::DecodeStats, usize), String> {
    let protected_capacity = carrier::capacity_bytes(image);
    let protected_carrier = carrier::extract_bytes(image, protected_capacity, quantization_step)?;

    let protected_header_len = ecc::encoded_header_len(frame::HEADER_LEN)?;
    let protected_header = protected_carrier
        .get(..protected_header_len)
        .ok_or_else(|| "carrier is too small for the protected BareSteg header".to_string())?;

    let (mut header, header_stats) =
        ecc::decode_header_with_stats(protected_header, frame::HEADER_LEN)?;

    let identity_repairs = frame::repair_header_identity(&mut header)?;

    if identity_repairs > MAX_HEADER_IDENTITY_REPAIRS {
        return Err(format!(
            "BareSteg header identity required {identity_repairs} repaired bits; maximum allowed is {MAX_HEADER_IDENTITY_REPAIRS}"
        ));
    }

    let mut candidates = Vec::new();
    let mut payload_len = 0_usize;

    loop {
        let protected_frame_len = ecc::encoded_frame_len(frame::HEADER_LEN, payload_len)?;

        if protected_frame_len > protected_capacity {
            break;
        }

        let mut candidate_header = header.clone();
        let length_repairs = frame::repair_payload_len(&mut candidate_header, payload_len)?;

        if length_repairs <= MAX_PAYLOAD_LENGTH_REPAIRS {
            candidates.push((
                length_repairs,
                payload_len,
                protected_frame_len,
                candidate_header,
            ));
        }

        payload_len = payload_len
            .checked_add(1)
            .ok_or_else(|| "payload length candidate overflowed this platform".to_string())?;
    }

    candidates.sort_by_key(|candidate| (candidate.0, candidate.1));

    let mut last_error = None;

    for (length_repairs, payload_len, protected_frame_len, candidate_header) in candidates {
        let protected_frame = &protected_carrier[..protected_frame_len];

        let (mut recovered_frame, recovery_stats) =
            ecc::decode_frame_with_stats(protected_frame, frame::HEADER_LEN, payload_len)?;

        let crc_repairs = {
            let (recovered_header, recovered_payload) =
                recovered_frame.split_at_mut(frame::HEADER_LEN);

            recovered_header.copy_from_slice(&candidate_header);

            frame::repair_payload_crc(recovered_header, recovered_payload)?
        };

        if crc_repairs > MAX_PAYLOAD_CRC_REPAIRS {
            last_error = Some(format!(
                "payload CRC required {crc_repairs} repaired bits; maximum allowed is {MAX_PAYLOAD_CRC_REPAIRS}"
            ));

            continue;
        }

        let payload = frame::decode(&recovered_frame)?;

        return Ok((
            payload,
            header_stats,
            recovery_stats,
            identity_repairs + length_repairs + crc_repairs,
        ));
    }

    Err(last_error.unwrap_or_else(|| {
        format!("no payload candidate survived bounded identity, length, and CRC repair")
    }))
}

fn usage() -> String {
    [
        "usage:",
        "  baresteg hide <carrier.bmp> <payload> <output.bmp>",
        "  baresteg reveal <image.bmp> <output>",
    ]
    .join("\n")
}
