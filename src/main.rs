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

type RecoveryResult = (
    Vec<u8>,
    ecc::DecodeStats,
    ecc::DecodeStats,
    usize,
    usize,
    usize,
    usize,
    usize,
);

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
        for pixel_weighted in [false, true] {
            match recover_with_quantization_step(&image, quantization_step, pixel_weighted) {
                Ok((
                    payload,
                    header_stats,
                    recovery_stats,
                    header_repairs,
                    header_copies,
                    crc_alternate_bits,
                    payload_copies,
                    payload_alternate_bits,
                )) => {
                    println!("QIM decode step: {quantization_step}");
                    println!(
                        "Carrier weighting: {}",
                        if pixel_weighted {
                            "pixel-weighted"
                        } else {
                            "equal-block"
                        }
                    );
                    println!("Header copies used: {header_copies}");
                    println!("Header CRC alternate bits: {crc_alternate_bits}");
                    println!("Payload copies used: {payload_copies}");
                    println!("Payload alternate bits: {payload_alternate_bits}");
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
    }

    Err(format!(
        "failed to recover BareSteg frame with all QIM and weighting candidates; last error: {}",
        last_error.unwrap_or_else(|| "no decoder candidate was attempted".to_string())
    ))
}

fn recover_with_quantization_step(
    image: &Bmp,
    quantization_step: i32,
    pixel_weighted: bool,
) -> Result<RecoveryResult, String> {
    let protected_capacity = carrier::capacity_bytes(image);

    let protected_carrier = if pixel_weighted {
        carrier::extract_bytes_pixel_weighted(image, protected_capacity, quantization_step)?
    } else {
        carrier::extract_bytes(image, protected_capacity, quantization_step)?
    };

    let protected_header_len = ecc::encoded_header_len(frame::HEADER_LEN)?;

    let protected_header = protected_carrier
        .get(..protected_header_len)
        .ok_or_else(|| "carrier is too small for the protected BareSteg header".to_string())?;

    let header_candidates =
        ecc::decode_header_candidates_with_stats(protected_header, frame::HEADER_LEN)?;

    let crc_candidates = ecc::header_crc_candidates(protected_header, frame::HEADER_LEN)?;

    let mut last_error = None;

    for (mut header, header_stats, header_copies) in header_candidates {
        let identity_repairs = frame::repair_header_identity(&mut header)?;

        if identity_repairs > MAX_HEADER_IDENTITY_REPAIRS {
            last_error = Some(format!(
                "BareSteg header identity required {identity_repairs} repaired bits; maximum allowed is {MAX_HEADER_IDENTITY_REPAIRS}"
            ));

            continue;
        }

        let mut length_candidates = Vec::new();
        let mut payload_len = 0_usize;

        loop {
            let protected_frame_len = ecc::encoded_frame_len(frame::HEADER_LEN, payload_len)?;

            if protected_frame_len > protected_capacity {
                break;
            }

            let mut candidate_header = header.clone();

            let length_repairs = frame::repair_payload_len(&mut candidate_header, payload_len)?;

            if length_repairs <= MAX_PAYLOAD_LENGTH_REPAIRS {
                length_candidates.push((
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

        length_candidates.sort_by_key(|candidate| (candidate.0, candidate.1));

        for (length_repairs, payload_len, protected_frame_len, candidate_header) in
            length_candidates
        {
            let protected_frame = &protected_carrier[..protected_frame_len];

            let disputed_payload_bits =
                ecc::payload_disputed_bits(protected_frame, frame::HEADER_LEN, payload_len)?;

            let frame_candidates = ecc::decode_frame_candidates_with_stats(
                protected_frame,
                frame::HEADER_LEN,
                payload_len,
            )?;

            for (mut recovered_frame, recovery_stats, payload_copies) in frame_candidates {
                recovered_frame[..frame::HEADER_LEN].copy_from_slice(&candidate_header);

                if let Some((payload, crc_alternate_bits)) =
                    decode_if_crc_matches(&mut recovered_frame, &crc_candidates)?
                {
                    return Ok((
                        payload,
                        header_stats,
                        recovery_stats,
                        identity_repairs + length_repairs,
                        header_copies,
                        crc_alternate_bits,
                        payload_copies,
                        0,
                    ));
                }

                if payload_copies > 1 {
                    for (first_index, &first_bit) in disputed_payload_bits.iter().enumerate() {
                        toggle_frame_payload_bit(&mut recovered_frame, first_bit);

                        if let Some((payload, crc_alternate_bits)) =
                            decode_if_crc_matches(&mut recovered_frame, &crc_candidates)?
                        {
                            return Ok((
                                payload,
                                header_stats,
                                recovery_stats,
                                identity_repairs + length_repairs,
                                header_copies,
                                crc_alternate_bits,
                                payload_copies,
                                1,
                            ));
                        }

                        for &second_bit in &disputed_payload_bits[first_index + 1..] {
                            toggle_frame_payload_bit(&mut recovered_frame, second_bit);

                            if let Some((payload, crc_alternate_bits)) =
                                decode_if_crc_matches(&mut recovered_frame, &crc_candidates)?
                            {
                                return Ok((
                                    payload,
                                    header_stats,
                                    recovery_stats,
                                    identity_repairs + length_repairs,
                                    header_copies,
                                    crc_alternate_bits,
                                    payload_copies,
                                    2,
                                ));
                            }

                            toggle_frame_payload_bit(&mut recovered_frame, second_bit);
                        }

                        toggle_frame_payload_bit(&mut recovered_frame, first_bit);
                    }
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        "no header, length, stored CRC, or payload candidate survived recovery".to_string()
    }))
}

fn decode_if_crc_matches(
    recovered_frame: &mut [u8],
    crc_candidates: &[([u8; 4], usize)],
) -> Result<Option<(Vec<u8>, usize)>, String> {
    let actual_crc = crc32::compute(&recovered_frame[frame::HEADER_LEN..]).to_le_bytes();

    for &(crc, alternate_bits) in crc_candidates {
        if crc != actual_crc {
            continue;
        }

        recovered_frame[17..21].copy_from_slice(&crc);

        let payload = frame::decode(recovered_frame)?;

        return Ok(Some((payload, alternate_bits)));
    }

    Ok(None)
}

fn toggle_frame_payload_bit(recovered_frame: &mut [u8], payload_bit_index: usize) {
    let byte_index = frame::HEADER_LEN + payload_bit_index / 8;

    recovered_frame[byte_index] ^= 1_u8 << (7 - payload_bit_index % 8);
}

fn usage() -> String {
    [
        "usage:",
        "  baresteg hide <carrier.bmp> <payload> <output.bmp>",
        "  baresteg reveal <image.bmp> <output>",
    ]
    .join("\n")
}
