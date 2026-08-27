pub fn compute(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;

    for &byte in bytes {
        crc ^= u32::from(byte);

        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }

    !crc
}

#[cfg(test)]
mod tests {
    use super::compute;

    #[test]
    fn crc32_matches_standard_check_vector() {
        assert_eq!(compute(b"123456789"), 0xcbf4_3926);
    }
}
