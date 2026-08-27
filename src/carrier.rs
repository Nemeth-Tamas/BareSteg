use crate::bmp::Bmp;

const CELL_SIZE: usize = 8;
const HALF_CELL: usize = CELL_SIZE / 2;
const TARGET_DIFFERENCE: i32 = 40;
const ADJUSTMENT_STEP: i16 = 4;
const MAX_ADJUSTMENT_PASSES: usize = 80;

pub fn embed(image: &mut Bmp, data: &[u8]) -> Result<(), String> {
    let required_bits = data
        .len()
        .checked_mul(8)
        .ok_or_else(|| "payload bit count overflowed this platform".to_string())?;

    ensure_capacity(image, required_bits)?;

    for bit_index in 0..required_bits {
        let byte = data[bit_index / 8];
        let shift = 7 - (bit_index % 8);
        let bit = ((byte >> shift) & 1) != 0;

        write_cell_bit(image, bit_index, bit)?;
    }

    Ok(())
}

pub fn extract_bytes(image: &Bmp, byte_count: usize) -> Result<Vec<u8>, String> {
    let required_bits = byte_count
        .checked_mul(8)
        .ok_or_else(|| "requested extraction size overflowed this platform".to_string())?;

    ensure_capacity(image, required_bits)?;

    let mut output = vec![0_u8; byte_count];

    for bit_index in 0..required_bits {
        if read_cell_bit(image, bit_index) {
            output[bit_index / 8] |= 1 << (7 - (bit_index % 8));
        }
    }

    Ok(output)
}

fn ensure_capacity(image: &Bmp, required_bits: usize) -> Result<(), String> {
    let available_bits = capacity_bits(image);

    if required_bits > available_bits {
        return Err(format!(
            "carrier is too small: need {required_bits} logical bits but image provides {available_bits}"
        ));
    }

    Ok(())
}

fn capacity_bits(image: &Bmp) -> usize {
    (image.width() / CELL_SIZE) * (image.height() / CELL_SIZE)
}

fn write_cell_bit(image: &mut Bmp, cell_index: usize, bit: bool) -> Result<(), String> {
    let (origin_x, origin_y) = cell_origin(image, cell_index);

    for _ in 0..MAX_ADJUSTMENT_PASSES {
        let difference = cell_difference(image, origin_x, origin_y);
        let encoded_difference = if bit { difference } else { -difference };

        if encoded_difference >= TARGET_DIFFERENCE {
            return Ok(());
        }

        if bit {
            adjust_half(image, origin_x, origin_y, true, ADJUSTMENT_STEP);
            adjust_half(image, origin_x, origin_y, false, -ADJUSTMENT_STEP);
        } else {
            adjust_half(image, origin_x, origin_y, true, -ADJUSTMENT_STEP);
            adjust_half(image, origin_x, origin_y, false, ADJUSTMENT_STEP);
        }
    }

    Err(format!(
        "failed to establish a reliable luminance difference in carrier cell {cell_index}"
    ))
}

fn read_cell_bit(image: &Bmp, cell_index: usize) -> bool {
    let (origin_x, origin_y) = cell_origin(image, cell_index);

    cell_difference(image, origin_x, origin_y) >= 0
}

fn cell_origin(image: &Bmp, cell_index: usize) -> (usize, usize) {
    let cells_per_row = image.width() / CELL_SIZE;

    (
        (cell_index % cells_per_row) * CELL_SIZE,
        (cell_index / cells_per_row) * CELL_SIZE,
    )
}

fn cell_difference(image: &Bmp, origin_x: usize, origin_y: usize) -> i32 {
    let mut left_sum = 0_u32;
    let mut right_sum = 0_u32;

    for y in origin_y..origin_y + CELL_SIZE {
        for x in origin_x..origin_x + HALF_CELL {
            left_sum += image.luminance(x, y);
        }

        for x in origin_x + HALF_CELL..origin_x + CELL_SIZE {
            right_sum += image.luminance(x, y);
        }
    }

    let pixels_per_half = (HALF_CELL * CELL_SIZE) as u32;
    let left_average = left_sum / pixels_per_half;
    let right_average = right_sum / pixels_per_half;

    left_average as i32 - right_average as i32
}

fn adjust_half(image: &mut Bmp, origin_x: usize, origin_y: usize, left_half: bool, delta: i16) {
    let start_x = if left_half {
        origin_x
    } else {
        origin_x + HALF_CELL
    };

    for y in origin_y..origin_y + CELL_SIZE {
        for x in start_x..start_x + HALF_CELL {
            image.adjust_pixel(x, y, delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{embed, extract_bytes};
    use crate::bmp::Bmp;

    #[test]
    fn carrier_roundtrip_recovers_exact_bytes() {
        let mut image = Bmp::test_image(64, 64);
        let payload = [0x00, 0x55, 0xaa, 0xff, 0x42, 0x19, 0x81, 0x7e];

        embed(&mut image, &payload).expect("embedding should succeed");

        let recovered = extract_bytes(&image, payload.len()).expect("extraction should succeed");

        assert_eq!(recovered, payload.to_vec());
    }
}
