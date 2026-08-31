use crate::crc32;

const MAGIC: &[u8; 8] = b"BARESTG1";
const VERSION: u8 = 1;

pub const HEADER_LEN: usize = 8 + 1 + 8 + 4;

pub fn encode(payload: &[u8]) -> Result<Vec<u8>, String> {
    let payload_len = u64::try_from(payload.len())
        .map_err(|_| "payload is too large for the BareSteg frame format".to_string())?;

    let mut frame = Vec::with_capacity(
        HEADER_LEN
            .checked_add(payload.len())
            .ok_or_else(|| "BareSteg frame size overflowed this platform".to_string())?,
    );

    frame.extend_from_slice(MAGIC);
    frame.push(VERSION);
    frame.extend_from_slice(&payload_len.to_le_bytes());
    frame.extend_from_slice(&crc32::compute(payload).to_le_bytes());
    frame.extend_from_slice(payload);

    Ok(frame)
}

pub fn repair_header_identity(header: &mut [u8]) -> Result<usize, String> {
    if header.len() < HEADER_LEN {
        return Err(format!(
            "BareSteg frame is too short: need at least {HEADER_LEN} bytes"
        ));
    }

    let mut repaired_bits = 0_usize;

    for (index, expected) in MAGIC.iter().enumerate() {
        repaired_bits += (header[index] ^ *expected).count_ones() as usize;
        header[index] = *expected;
    }

    repaired_bits += (header[8] ^ VERSION).count_ones() as usize;
    header[8] = VERSION;

    Ok(repaired_bits)
}

pub fn payload_len_from_header(header: &[u8]) -> Result<usize, String> {
    validate_header(header)?;

    let raw_length: [u8; 8] = header[9..17]
        .try_into()
        .map_err(|_| "BareSteg payload length field is malformed".to_string())?;

    let payload_len = u64::from_le_bytes(raw_length);

    usize::try_from(payload_len)
        .map_err(|_| "BareSteg payload length does not fit this platform".to_string())
}

pub fn decode(frame: &[u8]) -> Result<Vec<u8>, String> {
    let payload_len = payload_len_from_header(frame)?;

    let expected_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or_else(|| "BareSteg frame length overflowed this platform".to_string())?;

    if frame.len() != expected_len {
        return Err(format!(
            "BareSteg frame length mismatch: header expects {expected_len} bytes, recovered {}",
            frame.len()
        ));
    }

    let raw_crc: [u8; 4] = frame[17..21]
        .try_into()
        .map_err(|_| "BareSteg CRC32 field is malformed".to_string())?;

    let expected_crc = u32::from_le_bytes(raw_crc);
    let payload = &frame[HEADER_LEN..];
    let actual_crc = crc32::compute(payload);

    if actual_crc != expected_crc {
        return Err(format!(
            "BareSteg payload CRC32 mismatch: expected {expected_crc:08x}, got {actual_crc:08x}"
        ));
    }

    Ok(payload.to_vec())
}

fn validate_header(frame: &[u8]) -> Result<(), String> {
    if frame.len() < HEADER_LEN {
        return Err(format!(
            "BareSteg frame is too short: need at least {HEADER_LEN} bytes"
        ));
    }

    if &frame[0..8] != MAGIC {
        return Err("BareSteg synchronization marker was not found".to_string());
    }

    if frame[8] != VERSION {
        return Err(format!("unsupported BareSteg frame version: {}", frame[8]));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{HEADER_LEN, decode, encode, payload_len_from_header, repair_header_identity};

    #[test]
    fn frame_roundtrip_recovers_payload() {
        let payload = b"BareSteg frame test\0with binary\xff";
        let frame = encode(payload).expect("encoding should succeed");

        assert_eq!(
            payload_len_from_header(&frame[..HEADER_LEN]).expect("header should decode"),
            payload.len()
        );

        assert_eq!(
            decode(&frame).expect("frame should decode"),
            payload.to_vec()
        );
    }

    #[test]
    fn known_header_identity_can_be_repaired() {
        let mut frame = encode(b"payload").expect("encoding should succeed");

        frame[0] ^= 0x01;
        frame[3] ^= 0x20;
        frame[8] ^= 0x04;

        let repaired_bits =
            repair_header_identity(&mut frame).expect("header repair should succeed");

        assert_eq!(repaired_bits, 3);
        assert_eq!(
            payload_len_from_header(&frame[..HEADER_LEN]).expect("repaired header should validate"),
            7
        );
    }

    #[test]
    fn frame_rejects_corrupted_payload() {
        let mut frame = encode(b"payload").expect("encoding should succeed");
        let last = frame.len() - 1;

        frame[last] ^= 0x01;

        assert!(decode(&frame).is_err());
    }
}
