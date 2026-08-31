use crate::bmp::Bmp;

const GRID_COLUMNS: usize = 80;
const GRID_ROWS: usize = 40;
const BLOCKS_PER_SIDE: usize = 4;
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

pub fn extract_bytes(
    image: &Bmp,
    byte_count: usize,
    quantization_step: i32,
) -> Result<Vec<u8>, String> {
    extract_bytes_with_reader(image, byte_count, quantization_step, read_cell_bit)
}

pub fn extract_bytes_pixel_weighted(
    image: &Bmp,
    byte_count: usize,
    quantization_step: i32,
) -> Result<Vec<u8>, String> {
    extract_bytes_with_reader(
        image,
        byte_count,
        quantization_step,
        read_cell_bit_pixel_weighted,
    )
}

fn extract_bytes_with_reader(
    image: &Bmp,
    byte_count: usize,
    quantization_step: i32,
    read_bit: fn(&Bmp, usize, i32) -> bool,
) -> Result<Vec<u8>, String> {
    let required_bits = byte_count
        .checked_mul(8)
        .ok_or_else(|| "requested extraction size overflowed this platform".to_string())?;

    ensure_capacity(image, required_bits)?;

    let mut output = vec![0_u8; byte_count];

    for bit_index in 0..required_bits {
        if read_bit(image, bit_index, quantization_step) {
            output[bit_index / 8] |= 1 << (7 - (bit_index % 8));
        }
    }

    Ok(output)
}

fn ensure_capacity(image: &Bmp, required_bits: usize) -> Result<(), String> {
    let minimum_width = GRID_COLUMNS * BLOCKS_PER_SIDE;
    let minimum_height = GRID_ROWS * BLOCKS_PER_SIDE;

    if image.width() < minimum_width || image.height() < minimum_height {
        return Err(format!(
            "carrier is too small for normalized layout: image is {}x{}, need at least {minimum_width}x{minimum_height}",
            image.width(),
            image.height()
        ));
    }

    let available_bits = capacity_bits(image);

    if required_bits > available_bits {
        return Err(format!(
            "carrier is too small: need {required_bits} logical bits but image provides {available_bits}"
        ));
    }

    Ok(())
}

pub fn capacity_bytes(image: &Bmp) -> usize {
    capacity_bits(image) / 8
}

fn capacity_bits(_: &Bmp) -> usize {
    GRID_COLUMNS * GRID_ROWS
}

fn write_cell_bit(image: &mut Bmp, cell_index: usize, bit: bool) -> Result<(), String> {
    let (start_x, start_y, end_x, end_y) = cell_bounds(image, cell_index);
    let mask = carrier_mask(cell_index);
    let initial_correlation = cell_correlation(image, start_x, start_y, end_x, end_y, mask);
    let target = quantization_target(initial_correlation, bit);

    for _ in 0..MAX_ADJUSTMENT_PASSES {
        let correlation = cell_correlation(image, start_x, start_y, end_x, end_y, mask);
        let error = target - correlation;

        if error.abs() <= 1 {
            return Ok(());
        }

        let direction = if error > 0 {
            ADJUSTMENT_STEP
        } else {
            -ADJUSTMENT_STEP
        };

        adjust_pattern(image, start_x, start_y, end_x, end_y, mask, direction);
    }

    Err(format!(
        "failed to quantize carrier cell {cell_index} to target correlation {target}"
    ))
}

fn read_cell_bit(image: &Bmp, cell_index: usize, quantization_step: i32) -> bool {
    let (start_x, start_y, end_x, end_y) = cell_bounds(image, cell_index);
    let mask = carrier_mask(cell_index);
    let correlation = cell_correlation(image, start_x, start_y, end_x, end_y, mask);

    decode_difference(correlation, quantization_step)
}

fn read_cell_bit_pixel_weighted(image: &Bmp, cell_index: usize, quantization_step: i32) -> bool {
    let (start_x, start_y, end_x, end_y) = cell_bounds(image, cell_index);
    let mask = carrier_mask(cell_index);
    let correlation = pixel_weighted_correlation(image, start_x, start_y, end_x, end_y, mask);

    decode_difference(correlation, quantization_step)
}

fn cell_bounds(image: &Bmp, cell_index: usize) -> (usize, usize, usize, usize) {
    let (grid_x, grid_y) = cell_grid_position(cell_index);

    let start_x = rounded_partition(grid_x, image.width(), GRID_COLUMNS);
    let end_x = rounded_partition(grid_x + 1, image.width(), GRID_COLUMNS);
    let start_y = rounded_partition(grid_y, image.height(), GRID_ROWS);
    let end_y = rounded_partition(grid_y + 1, image.height(), GRID_ROWS);

    (start_x, start_y, end_x, end_y)
}

fn rounded_partition(index: usize, span: usize, partitions: usize) -> usize {
    let numerator = index as u128 * span as u128;
    let denominator = partitions as u128;

    ((numerator + denominator / 2) / denominator) as usize
}

fn cell_grid_position(cell_index: usize) -> (usize, usize) {
    let total_cells = GRID_COLUMNS * GRID_ROWS;
    let physical_index = permuted_cell_index(total_cells, cell_index);

    (physical_index % GRID_COLUMNS, physical_index / GRID_COLUMNS)
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

fn cell_correlation(
    image: &Bmp,
    start_x: usize,
    start_y: usize,
    end_x: usize,
    end_y: usize,
    mask: u16,
) -> i32 {
    let width = end_x - start_x;
    let height = end_y - start_y;

    let mut positive_sum = 0_u64;
    let mut negative_sum = 0_u64;
    let mut positive_blocks = 0_u64;
    let mut negative_blocks = 0_u64;

    for block_y in 0..BLOCKS_PER_SIDE {
        let block_start_y = start_y + block_y * height / BLOCKS_PER_SIDE;
        let block_end_y = start_y + (block_y + 1) * height / BLOCKS_PER_SIDE;

        for block_x in 0..BLOCKS_PER_SIDE {
            let block_start_x = start_x + block_x * width / BLOCKS_PER_SIDE;
            let block_end_x = start_x + (block_x + 1) * width / BLOCKS_PER_SIDE;
            let block_index = block_y * BLOCKS_PER_SIDE + block_x;

            let mut block_sum = 0_u64;
            let mut block_pixels = 0_u64;

            for y in block_start_y..block_end_y {
                for x in block_start_x..block_end_x {
                    block_sum += u64::from(image.luminance(x, y));
                    block_pixels += 1;
                }
            }

            let block_average = block_sum / block_pixels;

            if mask & (1_u16 << block_index) != 0 {
                positive_sum += block_average;
                positive_blocks += 1;
            } else {
                negative_sum += block_average;
                negative_blocks += 1;
            }
        }
    }

    let positive_average = positive_sum / positive_blocks;
    let negative_average = negative_sum / negative_blocks;

    positive_average as i32 - negative_average as i32
}

fn pixel_weighted_correlation(
    image: &Bmp,
    start_x: usize,
    start_y: usize,
    end_x: usize,
    end_y: usize,
    mask: u16,
) -> i32 {
    let width = end_x - start_x;
    let height = end_y - start_y;

    let mut positive_sum = 0_u64;
    let mut negative_sum = 0_u64;
    let mut positive_count = 0_u64;
    let mut negative_count = 0_u64;

    for y in start_y..end_y {
        for x in start_x..end_x {
            let local_x = x - start_x;
            let local_y = y - start_y;
            let block_x = local_x * BLOCKS_PER_SIDE / width;
            let block_y = local_y * BLOCKS_PER_SIDE / height;
            let block_index = block_y * BLOCKS_PER_SIDE + block_x;
            let luminance = u64::from(image.luminance(x, y));

            if mask & (1_u16 << block_index) != 0 {
                positive_sum += luminance;
                positive_count += 1;
            } else {
                negative_sum += luminance;
                negative_count += 1;
            }
        }
    }

    let positive_average = positive_sum / positive_count;
    let negative_average = negative_sum / negative_count;

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
    let nearest = nearest_bucket(difference, QUANTIZATION_STEP);

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

fn nearest_bucket(difference: i32, quantization_step: i32) -> i32 {
    let half_step = quantization_step / 2;

    if difference >= 0 {
        (difference + half_step) / quantization_step
    } else {
        (difference - half_step) / quantization_step
    }
}

fn decode_difference(difference: i32, quantization_step: i32) -> bool {
    nearest_bucket(difference, quantization_step).rem_euclid(2) != 0
}

fn adjust_pattern(
    image: &mut Bmp,
    start_x: usize,
    start_y: usize,
    end_x: usize,
    end_y: usize,
    mask: u16,
    direction: i16,
) {
    let width = end_x - start_x;
    let height = end_y - start_y;

    for y in start_y..end_y {
        for x in start_x..end_x {
            let local_x = x - start_x;
            let local_y = y - start_y;
            let block_x = local_x * BLOCKS_PER_SIDE / width;
            let block_y = local_y * BLOCKS_PER_SIDE / height;
            let block_index = block_y * BLOCKS_PER_SIDE + block_x;

            let delta = if mask & (1_u16 << block_index) != 0 {
                direction
            } else {
                -direction
            };

            image.adjust_pixel(x, y, delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BLOCK_COUNT, QUANTIZATION_STEP, carrier_mask, cell_bounds, decode_difference, embed,
        extract_bytes, quantization_target, rounded_partition,
    };
    use crate::bmp::Bmp;

    #[test]
    fn carrier_roundtrip_recovers_exact_bytes() {
        let mut image = Bmp::test_image(640, 480);
        let payload = [0x00, 0x55, 0xaa, 0xff, 0x42, 0x19, 0x81, 0x7e];

        embed(&mut image, &payload).expect("embedding should succeed");

        let recovered = extract_bytes(&image, payload.len(), QUANTIZATION_STEP)
            .expect("extraction should succeed");

        assert_eq!(recovered, payload.to_vec());
    }

    #[test]
    fn normalized_cell_bounds_scale_with_image() {
        let small = Bmp::test_image(640, 480);
        let large = Bmp::test_image(1280, 960);

        for cell_index in [0, 1, 317, 2048, 3199] {
            let small_bounds = cell_bounds(&small, cell_index);
            let large_bounds = cell_bounds(&large, cell_index);

            assert_eq!(large_bounds.0, small_bounds.0 * 2);
            assert_eq!(large_bounds.1, small_bounds.1 * 2);
            assert_eq!(large_bounds.2, small_bounds.2 * 2);
            assert_eq!(large_bounds.3, small_bounds.3 * 2);
        }
    }

    #[test]
    fn fractional_normalized_boundaries_round_to_nearest_pixel() {
        assert_eq!(rounded_partition(1, 1920, 80), 24);
        assert_eq!(rounded_partition(1, 1280, 80), 16);

        assert_eq!(rounded_partition(1, 1824, 80), 23);
        assert_eq!(rounded_partition(2, 1824, 80), 46);

        assert_eq!(rounded_partition(1, 1728, 80), 22);
        assert_eq!(rounded_partition(2, 1728, 80), 43);
        assert_eq!(rounded_partition(3, 1728, 80), 65);

        assert_eq!(rounded_partition(1, 1152, 80), 14);
        assert_eq!(rounded_partition(2, 1152, 80), 29);
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

                assert_eq!(decode_difference(target, QUANTIZATION_STEP), bit);
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
