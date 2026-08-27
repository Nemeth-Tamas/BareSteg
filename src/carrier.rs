use crate::bmp::Bmp;

const CELL_SIZE: usize = 16;
const BLOCK_SIZE: usize = 4;
const BLOCKS_PER_SIDE: usize = CELL_SIZE / BLOCK_SIZE;
const BLOCK_COUNT: usize = BLOCKS_PER_SIDE * BLOCKS_PER_SIDE;
const QUANTIZATION_STEP: i32 = 8;
const ADJUSTMENT_STEP: i16 = 1;
const MAX_ADJUSTMENT_PASSES: usize = 12;

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
    let mask = carrier_mask(cell_index);
    let initial_correlation = cell_correlation(image, origin_x, origin_y, mask);
    let target = quantization_target(initial_correlation, bit);

    for _ in 0..MAX_ADJUSTMENT_PASSES {
        let correlation = cell_correlation(image, origin_x, origin_y, mask);
        let error = target - correlation;

        if error.abs() <= 1 {
            return Ok(());
        }

        let direction = if error > 0 {
            ADJUSTMENT_STEP
        } else {
            -ADJUSTMENT_STEP
        };

        adjust_pattern(image, origin_x, origin_y, mask, direction);
    }

    Err(format!(
        "failed to quantize carrier cell {cell_index} to target correlation {target}"
    ))
}

fn read_cell_bit(image: &Bmp, cell_index: usize) -> bool {
    let (origin_x, origin_y) = cell_origin(image, cell_index);
    let mask = carrier_mask(cell_index);
    let correlation = cell_correlation(image, origin_x, origin_y, mask);

    decode_difference(correlation)
}

fn cell_origin(image: &Bmp, cell_index: usize) -> (usize, usize) {
    let cells_per_row = image.width() / CELL_SIZE;
    let total_cells = capacity_bits(image);
    let physical_index = permuted_cell_index(total_cells, cell_index);

    (
        (physical_index % cells_per_row) * CELL_SIZE,
        (physical_index / cells_per_row) * CELL_SIZE,
    )
}

fn permuted_cell_index(total_cells: usize, logical_index: usize) -> usize {
    if total_cells <= 1 {
        return 0;
    }

    let step = permutation_step(total_cells);
    let offset = total_cells / 7;

    ((logical_index as u128 * step as u128 + offset as u128) % total_cells as u128) as usize
}

fn permutation_step(total_cells: usize) -> usize {
    let mut step = ((total_cells as u128 * 618_033_989_u128) / 1_000_000_000_u128) as usize;

    step = step.max(1);

    while greatest_common_divisor(step, total_cells) != 1 {
        step += 1;

        if step >= total_cells {
            step = 1;
        }
    }

    step
}

fn greatest_common_divisor(mut left: usize, mut right: usize) -> usize {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }

    left
}

fn cell_correlation(image: &Bmp, origin_x: usize, origin_y: usize, mask: u16) -> i32 {
    let mut positive_sum = 0_u32;
    let mut negative_sum = 0_u32;

    for local_y in 0..CELL_SIZE {
        for local_x in 0..CELL_SIZE {
            let block_x = local_x / BLOCK_SIZE;
            let block_y = local_y / BLOCK_SIZE;
            let block_index = block_y * BLOCKS_PER_SIDE + block_x;
            let luminance = image.luminance(origin_x + local_x, origin_y + local_y);

            if mask & (1_u16 << block_index) != 0 {
                positive_sum += luminance;
            } else {
                negative_sum += luminance;
            }
        }
    }

    let pixels_per_group = (CELL_SIZE * CELL_SIZE / 2) as u32;
    let positive_average = positive_sum / pixels_per_group;
    let negative_average = negative_sum / pixels_per_group;

    positive_average as i32 - negative_average as i32
}

fn carrier_mask(cell_index: usize) -> u16 {
    let mut order = [0_usize; BLOCK_COUNT];

    for (index, slot) in order.iter_mut().enumerate() {
        *slot = index;
    }

    let mut state = (cell_index as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .wrapping_add(0xd1b5_4a32_d192_ed03);

    for upper in (1..BLOCK_COUNT).rev() {
        let swap_index = (next_random(&mut state) as usize) % (upper + 1);
        order.swap(upper, swap_index);
    }

    let mut mask = 0_u16;

    for &block_index in &order[..BLOCK_COUNT / 2] {
        mask |= 1_u16 << block_index;
    }

    mask
}

fn next_random(state: &mut u64) -> u64 {
    let mut value = *state;

    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;

    *state = value;

    value
}

fn quantization_target(difference: i32, bit: bool) -> i32 {
    let desired_parity = if bit { 1 } else { 0 };
    let nearest = nearest_bucket(difference);

    if nearest.rem_euclid(2) == desired_parity {
        return nearest * QUANTIZATION_STEP;
    }

    let lower = (nearest - 1) * QUANTIZATION_STEP;
    let upper = (nearest + 1) * QUANTIZATION_STEP;

    if difference.abs_diff(lower) <= difference.abs_diff(upper) {
        lower
    } else {
        upper
    }
}

fn nearest_bucket(difference: i32) -> i32 {
    let half_step = QUANTIZATION_STEP / 2;

    if difference >= 0 {
        (difference + half_step) / QUANTIZATION_STEP
    } else {
        (difference - half_step) / QUANTIZATION_STEP
    }
}

fn decode_difference(difference: i32) -> bool {
    nearest_bucket(difference).rem_euclid(2) != 0
}

fn adjust_pattern(image: &mut Bmp, origin_x: usize, origin_y: usize, mask: u16, direction: i16) {
    for local_y in 0..CELL_SIZE {
        for local_x in 0..CELL_SIZE {
            let block_x = local_x / BLOCK_SIZE;
            let block_y = local_y / BLOCK_SIZE;
            let block_index = block_y * BLOCKS_PER_SIDE + block_x;

            let delta = if mask & (1_u16 << block_index) != 0 {
                direction
            } else {
                -direction
            };

            image.adjust_pixel(origin_x + local_x, origin_y + local_y, delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BLOCK_COUNT, QUANTIZATION_STEP, carrier_mask, decode_difference, embed, extract_bytes,
        quantization_target,
    };
    use crate::bmp::Bmp;

    #[test]
    fn carrier_roundtrip_recovers_exact_bytes() {
        let mut image = Bmp::test_image(128, 128);
        let payload = [0x00, 0x55, 0xaa, 0xff, 0x42, 0x19, 0x81, 0x7e];

        embed(&mut image, &payload).expect("embedding should succeed");

        let recovered = extract_bytes(&image, payload.len()).expect("extraction should succeed");

        assert_eq!(recovered, payload.to_vec());
    }

    #[test]
    fn carrier_masks_are_balanced() {
        for cell_index in 0..512 {
            assert_eq!(
                carrier_mask(cell_index).count_ones(),
                (BLOCK_COUNT / 2) as u32
            );
        }
    }

    #[test]
    fn quantization_targets_encode_requested_bit() {
        for difference in -255..=255 {
            for bit in [false, true] {
                let target = quantization_target(difference, bit);

                assert_eq!(decode_difference(target), bit);
            }
        }
    }

    #[test]
    fn quantization_never_moves_more_than_one_step() {
        for difference in -255..=255 {
            for bit in [false, true] {
                let target = quantization_target(difference, bit);

                assert!(difference.abs_diff(target) <= QUANTIZATION_STEP as u32);
            }
        }
    }
}
