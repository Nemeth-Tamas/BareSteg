const HEADER_REPETITIONS: usize = 5;
const PAYLOAD_REPETITIONS: usize = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecodeStats {
    pub logical_bits: usize,
    pub protected_votes: usize,
    pub disputed_bits: usize,
    pub minority_votes: usize,
}

impl DecodeStats {
    fn combined(self, other: Self) -> Self {
        Self {
            logical_bits: self.logical_bits + other.logical_bits,
            protected_votes: self.protected_votes + other.protected_votes,
            disputed_bits: self.disputed_bits + other.disputed_bits,
            minority_votes: self.minority_votes + other.minority_votes,
        }
    }
}

pub fn encode_frame(frame: &[u8], header_len: usize) -> Result<Vec<u8>, String> {
    if frame.len() < header_len {
        return Err(format!(
            "frame is too short for ECC header: have {} bytes, need {header_len}",
            frame.len()
        ));
    }

    let encoded_header = encode_repeated(&frame[..header_len], HEADER_REPETITIONS)?;
    let encoded_payload = encode_repeated(&frame[header_len..], PAYLOAD_REPETITIONS)?;

    let total_len = encoded_header
        .len()
        .checked_add(encoded_payload.len())
        .ok_or_else(|| "ECC frame size overflowed this platform".to_string())?;

    let mut encoded = Vec::with_capacity(total_len);

    encoded.extend_from_slice(&encoded_header);
    encoded.extend_from_slice(&encoded_payload);

    Ok(encoded)
}

pub fn encoded_header_len(header_len: usize) -> Result<usize, String> {
    encoded_len(header_len, HEADER_REPETITIONS)
}

pub fn encoded_frame_len(header_len: usize, payload_len: usize) -> Result<usize, String> {
    encoded_header_len(header_len)?
        .checked_add(encoded_len(payload_len, PAYLOAD_REPETITIONS)?)
        .ok_or_else(|| "ECC frame size overflowed this platform".to_string())
}

pub fn decode_header_with_stats(
    encoded_header: &[u8],
    header_len: usize,
) -> Result<(Vec<u8>, DecodeStats), String> {
    decode_repeated_with_stats(encoded_header, header_len, HEADER_REPETITIONS)
}

pub fn decode_header_candidates_with_stats(
    encoded_header: &[u8],
    header_len: usize,
) -> Result<Vec<(Vec<u8>, DecodeStats, usize)>, String> {
    let expected_len = encoded_len(header_len, HEADER_REPETITIONS)?;

    if encoded_header.len() != expected_len {
        return Err(format!(
            "ECC header length mismatch: expected {expected_len} bytes, recovered {}",
            encoded_header.len()
        ));
    }

    let (header, stats) = decode_header_with_stats(encoded_header, header_len)?;

    let mut candidates = vec![(header, stats, HEADER_REPETITIONS)];
    let mask_limit = 1_u64 << HEADER_REPETITIONS;

    for copy_mask in 1_u64..mask_limit {
        if copy_mask.count_ones() != 3 {
            continue;
        }

        let (header, stats) = decode_repeated_copy_mask_with_stats(
            encoded_header,
            header_len,
            HEADER_REPETITIONS,
            copy_mask,
        )?;

        if candidates
            .iter()
            .any(|(existing, _, _)| existing == &header)
        {
            continue;
        }

        candidates.push((header, stats, 3));
    }

    Ok(candidates)
}

pub fn header_crc_candidates(
    encoded_header: &[u8],
    header_len: usize,
) -> Result<Vec<([u8; 4], usize)>, String> {
    const CRC_OFFSET: usize = 17;
    const CRC_LEN: usize = 4;

    let expected_len = encoded_len(header_len, HEADER_REPETITIONS)?;

    if encoded_header.len() != expected_len {
        return Err(format!(
            "ECC header length mismatch: expected {expected_len} bytes, recovered {}",
            encoded_header.len()
        ));
    }

    if header_len < CRC_OFFSET + CRC_LEN {
        return Err("ECC header is too short to contain BareSteg CRC32".to_string());
    }

    let source_bits = header_len
        .checked_mul(8)
        .ok_or_else(|| "ECC header bit count overflowed this platform".to_string())?;

    let mut majority_crc = [0_u8; CRC_LEN];
    let mut disputed_bits = Vec::new();

    for crc_bit_index in 0..CRC_LEN * 8 {
        let header_bit_index = CRC_OFFSET * 8 + crc_bit_index;
        let mut one_votes = 0_usize;

        for repetition in 0..HEADER_REPETITIONS {
            let repetition_offset = repetition
                .checked_mul(source_bits)
                .ok_or_else(|| "ECC repetition offset overflowed this platform".to_string())?;

            if bit_at(encoded_header, repetition_offset + header_bit_index) {
                one_votes += 1;
            }
        }

        let zero_votes = HEADER_REPETITIONS - one_votes;

        if one_votes > HEADER_REPETITIONS / 2 {
            set_bit(&mut majority_crc, crc_bit_index);
        }

        if one_votes != 0 && zero_votes != 0 {
            disputed_bits.push(crc_bit_index);
        }
    }

    let mut candidates = vec![(majority_crc, 0)];

    for (first_index, &first_bit) in disputed_bits.iter().enumerate() {
        let mut candidate = majority_crc;

        toggle_bit(&mut candidate, first_bit);
        candidates.push((candidate, 1));

        for &second_bit in &disputed_bits[first_index + 1..] {
            let mut candidate = majority_crc;

            toggle_bit(&mut candidate, first_bit);
            toggle_bit(&mut candidate, second_bit);

            candidates.push((candidate, 2));
        }
    }

    Ok(candidates)
}

pub fn decode_frame_with_stats(
    encoded_frame: &[u8],
    header_len: usize,
    payload_len: usize,
) -> Result<(Vec<u8>, DecodeStats), String> {
    let encoded_header_len = encoded_header_len(header_len)?;
    let expected_len = encoded_frame_len(header_len, payload_len)?;

    if encoded_frame.len() != expected_len {
        return Err(format!(
            "ECC frame length mismatch: expected {expected_len} bytes, recovered {}",
            encoded_frame.len()
        ));
    }

    let (header, header_stats) = decode_repeated_with_stats(
        &encoded_frame[..encoded_header_len],
        header_len,
        HEADER_REPETITIONS,
    )?;

    let (payload, payload_stats) = decode_repeated_with_stats(
        &encoded_frame[encoded_header_len..],
        payload_len,
        PAYLOAD_REPETITIONS,
    )?;

    let frame_len = header_len
        .checked_add(payload_len)
        .ok_or_else(|| "decoded frame size overflowed this platform".to_string())?;

    let mut frame = Vec::with_capacity(frame_len);

    frame.extend_from_slice(&header);
    frame.extend_from_slice(&payload);

    Ok((frame, header_stats.combined(payload_stats)))
}

pub fn decode_frame_candidates_with_stats(
    encoded_frame: &[u8],
    header_len: usize,
    payload_len: usize,
) -> Result<Vec<(Vec<u8>, DecodeStats, usize)>, String> {
    let encoded_header_len = encoded_header_len(header_len)?;
    let expected_len = encoded_frame_len(header_len, payload_len)?;

    if encoded_frame.len() != expected_len {
        return Err(format!(
            "ECC frame length mismatch: expected {expected_len} bytes, recovered {}",
            encoded_frame.len()
        ));
    }

    let (header, header_stats) = decode_repeated_with_stats(
        &encoded_frame[..encoded_header_len],
        header_len,
        HEADER_REPETITIONS,
    )?;

    let encoded_payload = &encoded_frame[encoded_header_len..];

    let (majority_payload, majority_stats) =
        decode_repeated_with_stats(encoded_payload, payload_len, PAYLOAD_REPETITIONS)?;

    let mut payload_candidates = vec![(majority_payload, majority_stats, PAYLOAD_REPETITIONS)];

    for repetition in 0..PAYLOAD_REPETITIONS {
        let copy_mask = 1_u64 << repetition;

        let (payload, stats) = decode_repeated_copy_mask_with_stats(
            encoded_payload,
            payload_len,
            PAYLOAD_REPETITIONS,
            copy_mask,
        )?;

        if payload_candidates
            .iter()
            .any(|(existing, _, _)| existing == &payload)
        {
            continue;
        }

        payload_candidates.push((payload, stats, 1));
    }

    let mut frame_candidates = Vec::with_capacity(payload_candidates.len());

    for (payload, payload_stats, copies_used) in payload_candidates {
        let mut frame = Vec::with_capacity(
            header_len
                .checked_add(payload_len)
                .ok_or_else(|| "decoded frame size overflowed this platform".to_string())?,
        );

        frame.extend_from_slice(&header);
        frame.extend_from_slice(&payload);

        frame_candidates.push((frame, header_stats.combined(payload_stats), copies_used));
    }

    Ok(frame_candidates)
}

fn encoded_len(source_len: usize, repetitions: usize) -> Result<usize, String> {
    source_len
        .checked_mul(repetitions)
        .ok_or_else(|| "ECC encoded size overflowed this platform".to_string())
}

fn encode_repeated(data: &[u8], repetitions: usize) -> Result<Vec<u8>, String> {
    let source_bits = data
        .len()
        .checked_mul(8)
        .ok_or_else(|| "ECC source bit count overflowed this platform".to_string())?;

    let mut encoded = vec![0_u8; encoded_len(data.len(), repetitions)?];

    for repetition in 0..repetitions {
        let repetition_offset = repetition
            .checked_mul(source_bits)
            .ok_or_else(|| "ECC repetition offset overflowed this platform".to_string())?;

        for bit_index in 0..source_bits {
            if bit_at(data, bit_index) {
                set_bit(&mut encoded, repetition_offset + bit_index);
            }
        }
    }

    Ok(encoded)
}

fn decode_repeated_with_stats(
    encoded: &[u8],
    source_len: usize,
    repetitions: usize,
) -> Result<(Vec<u8>, DecodeStats), String> {
    if repetitions >= u64::BITS as usize {
        return Err("ECC repetition count exceeds decoder mask capacity".to_string());
    }

    let copy_mask = (1_u64 << repetitions) - 1;

    decode_repeated_copy_mask_with_stats(encoded, source_len, repetitions, copy_mask)
}

fn decode_repeated_copy_mask_with_stats(
    encoded: &[u8],
    source_len: usize,
    total_repetitions: usize,
    copy_mask: u64,
) -> Result<(Vec<u8>, DecodeStats), String> {
    let expected_len = encoded_len(source_len, total_repetitions)?;

    if encoded.len() != expected_len {
        return Err(format!(
            "ECC block length mismatch: expected {expected_len} bytes, recovered {}",
            encoded.len()
        ));
    }

    let repetitions = copy_mask.count_ones() as usize;

    if repetitions == 0 || repetitions.is_multiple_of(2) {
        return Err("ECC copy-mask decoding requires a nonzero odd number of copies".to_string());
    }

    let source_bits = source_len
        .checked_mul(8)
        .ok_or_else(|| "ECC source bit count overflowed this platform".to_string())?;

    let protected_votes = source_bits
        .checked_mul(repetitions)
        .ok_or_else(|| "ECC protected vote count overflowed this platform".to_string())?;

    let mut decoded = vec![0_u8; source_len];

    let mut stats = DecodeStats {
        logical_bits: source_bits,
        protected_votes,
        ..DecodeStats::default()
    };

    for bit_index in 0..source_bits {
        let mut one_votes = 0_usize;

        for repetition in 0..total_repetitions {
            if copy_mask & (1_u64 << repetition) == 0 {
                continue;
            }

            let repetition_offset = repetition
                .checked_mul(source_bits)
                .ok_or_else(|| "ECC repetition offset overflowed this platform".to_string())?;

            if bit_at(encoded, repetition_offset + bit_index) {
                one_votes += 1;
            }
        }

        let zero_votes = repetitions - one_votes;

        if one_votes != 0 && zero_votes != 0 {
            stats.disputed_bits += 1;
            stats.minority_votes += one_votes.min(zero_votes);
        }

        if one_votes > repetitions / 2 {
            set_bit(&mut decoded, bit_index);
        }
    }

    Ok((decoded, stats))
}

fn bit_at(bytes: &[u8], bit_index: usize) -> bool {
    let byte = bytes[bit_index / 8];
    let mask = 1_u8 << (7 - bit_index % 8);

    byte & mask != 0
}

fn set_bit(bytes: &mut [u8], bit_index: usize) {
    bytes[bit_index / 8] |= 1_u8 << (7 - bit_index % 8);
}

fn toggle_bit(bytes: &mut [u8], bit_index: usize) {
    bytes[bit_index / 8] ^= 1_u8 << (7 - bit_index % 8);
}

#[cfg(test)]
mod tests {
    use super::{
        HEADER_REPETITIONS, PAYLOAD_REPETITIONS, decode_frame_candidates_with_stats,
        decode_frame_with_stats, decode_header_candidates_with_stats, decode_header_with_stats,
        decode_repeated_with_stats, encode_frame, encode_repeated, encoded_header_len,
        header_crc_candidates,
    };

    #[test]
    fn repetition_roundtrip_recovers_exact_bytes() {
        let data = b"BareSteg ECC test\x00\x55\xaa\xff";
        let encoded = encode_repeated(data, PAYLOAD_REPETITIONS).expect("encoding should succeed");

        let (decoded, _) = decode_repeated_with_stats(&encoded, data.len(), PAYLOAD_REPETITIONS)
            .expect("decoding should succeed");

        assert_eq!(decoded, data.to_vec());
    }

    #[test]
    fn majority_vote_recovers_when_one_entire_copy_is_destroyed() {
        let data = b"one copy may die";
        let mut encoded =
            encode_repeated(data, PAYLOAD_REPETITIONS).expect("encoding should succeed");

        for byte in &mut encoded[..data.len()] {
            *byte ^= 0xff;
        }

        let (decoded, stats) =
            decode_repeated_with_stats(&encoded, data.len(), PAYLOAD_REPETITIONS)
                .expect("majority decoding should succeed");

        let source_bits = data.len() * 8;

        assert_eq!(decoded, data.to_vec());
        assert_eq!(stats.logical_bits, source_bits);
        assert_eq!(stats.protected_votes, source_bits * PAYLOAD_REPETITIONS);
        assert_eq!(stats.disputed_bits, source_bits);
        assert_eq!(stats.minority_votes, source_bits);
    }

    #[test]
    fn header_can_be_recovered_before_payload_length_is_known() {
        let header = [0x5a_u8; 21];
        let payload = [0xa5_u8; 37];
        let mut frame = header.to_vec();

        frame.extend_from_slice(&payload);

        let encoded = encode_frame(&frame, header.len()).expect("frame encoding should succeed");
        let header_encoded_len = encoded_header_len(header.len()).expect("header size should fit");

        assert_eq!(header_encoded_len, header.len() * HEADER_REPETITIONS);

        let (recovered_header, _) =
            decode_header_with_stats(&encoded[..header_encoded_len], header.len())
                .expect("header should decode independently");

        assert_eq!(recovered_header, header.to_vec());
    }

    #[test]
    fn header_candidates_can_recover_when_five_copy_majority_is_wrong() {
        let header = [0x00_u8; 21];

        let mut encoded =
            encode_repeated(&header, HEADER_REPETITIONS).expect("encoding should succeed");

        for repetition in [1_usize, 2, 4] {
            let start = repetition * header.len();
            let end = start + header.len();

            for byte in &mut encoded[start..end] {
                *byte ^= 0xff;
            }
        }

        let (five_copy_header, _) = decode_header_with_stats(&encoded, header.len())
            .expect("five-copy header decoding should succeed");

        assert_ne!(five_copy_header, header);

        let candidates = decode_header_candidates_with_stats(&encoded, header.len())
            .expect("header candidate decoding should succeed");

        assert!(
            candidates
                .iter()
                .any(|(candidate, _, copies_used)| { candidate == &header && *copies_used == 3 })
        );
    }

    #[test]
    fn crc_candidates_can_recover_two_separate_minority_bits() {
        let header = [0x5a_u8; 21];

        let mut encoded =
            encode_repeated(&header, HEADER_REPETITIONS).expect("encoding should succeed");

        for repetition in [0_usize, 1, 2] {
            let start = repetition * header.len();

            encoded[start + 17] ^= 0x20;
        }

        for repetition in [1_usize, 3, 4] {
            let start = repetition * header.len();

            encoded[start + 19] ^= 0x01;
        }

        let (majority_header, _) = decode_header_with_stats(&encoded, header.len())
            .expect("header majority should decode");

        assert_ne!(&majority_header[17..21], &header[17..21]);

        let expected_crc: [u8; 4] = header[17..21].try_into().expect("CRC field should fit");

        let candidates =
            header_crc_candidates(&encoded, header.len()).expect("CRC candidates should decode");

        assert!(candidates.iter().any(|(candidate, alternate_bits)| {
            candidate == &expected_crc && *alternate_bits == 2
        }));
    }

    #[test]
    fn payload_candidates_can_recover_when_three_copy_majority_is_wrong() {
        let header = [0x42_u8; 21];
        let payload = [0x81_u8; 31];
        let mut frame = header.to_vec();

        frame.extend_from_slice(&payload);

        let mut encoded =
            encode_frame(&frame, header.len()).expect("frame encoding should succeed");

        let payload_start = header.len() * HEADER_REPETITIONS;

        for repetition in [1_usize, 2] {
            let start = payload_start + repetition * payload.len();
            let end = start + payload.len();

            for byte in &mut encoded[start..end] {
                *byte ^= 0xff;
            }
        }

        let (majority_frame, _) = decode_frame_with_stats(&encoded, header.len(), payload.len())
            .expect("majority frame decoding should succeed");

        assert_ne!(&majority_frame[header.len()..], payload);

        let candidates = decode_frame_candidates_with_stats(&encoded, header.len(), payload.len())
            .expect("payload candidate decoding should succeed");

        assert!(candidates.iter().any(|(candidate, _, copies_used)| {
            candidate[header.len()..] == payload && *copies_used == 1
        }));
    }

    #[test]
    fn protected_frame_roundtrip_recovers_exact_frame() {
        let header = [0x42_u8; 21];
        let payload = [0x81_u8; 31];
        let mut frame = header.to_vec();

        frame.extend_from_slice(&payload);

        let encoded = encode_frame(&frame, header.len()).expect("frame encoding should succeed");

        assert_eq!(
            encoded.len(),
            header.len() * HEADER_REPETITIONS + payload.len() * PAYLOAD_REPETITIONS
        );

        let (decoded, _) = decode_frame_with_stats(&encoded, header.len(), payload.len())
            .expect("protected frame should decode");

        assert_eq!(decoded, frame);
    }
}
