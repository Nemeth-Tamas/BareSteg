use std::fs;

pub struct Bmp {
    bytes: Vec<u8>,
    pixel_offset: usize,
    width: usize,
    height: usize,
    row_stride: usize,
    top_down: bool,
}

impl Bmp {
    pub fn load(path: &str) -> Result<Self, String> {
        let bytes =
            fs::read(path).map_err(|error| format!("failed to read BMP '{path}': {error}"))?;

        Self::from_bytes(bytes)
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        fs::write(path, &self.bytes)
            .map_err(|error| format!("failed to write BMP '{path}': {error}"))
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn luminance(&self, x: usize, y: usize) -> u32 {
        let index = self.pixel_index(x, y);
        let blue = u32::from(self.bytes[index]);
        let green = u32::from(self.bytes[index + 1]);
        let red = u32::from(self.bytes[index + 2]);

        (77 * red + 150 * green + 29 * blue) >> 8
    }

    pub fn adjust_pixel(&mut self, x: usize, y: usize, delta: i16) {
        let index = self.pixel_index(x, y);

        for channel in &mut self.bytes[index..index + 3] {
            *channel = (i16::from(*channel) + delta).clamp(0, 255) as u8;
        }
    }

    fn from_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.len() < 54 {
            return Err("BMP is too small to contain the required headers".to_string());
        }

        if !bytes.starts_with(b"BM") {
            return Err("unsupported image: expected Windows BMP signature 'BM'".to_string());
        }

        let pixel_offset = read_u32(&bytes, 10)? as usize;
        let dib_size = read_u32(&bytes, 14)?;

        if dib_size < 40 {
            return Err(format!(
                "unsupported BMP DIB header: {dib_size} bytes, need at least 40"
            ));
        }

        let width = read_i32(&bytes, 18)?;
        let signed_height = read_i32(&bytes, 22)?;
        let planes = read_u16(&bytes, 26)?;
        let bits_per_pixel = read_u16(&bytes, 28)?;
        let compression = read_u32(&bytes, 30)?;

        if width <= 0 {
            return Err("unsupported BMP width: must be positive".to_string());
        }

        if signed_height == 0 {
            return Err("unsupported BMP height: must not be zero".to_string());
        }

        if planes != 1 {
            return Err(format!("unsupported BMP plane count: {planes}"));
        }

        if bits_per_pixel != 24 {
            return Err(format!(
                "unsupported BMP bit depth: {bits_per_pixel}; BareSteg POC requires 24-bit BMP"
            ));
        }

        if compression != 0 {
            return Err(format!(
                "unsupported BMP compression mode: {compression}; BareSteg POC requires BI_RGB"
            ));
        }

        let width = width as usize;
        let height = signed_height.unsigned_abs() as usize;

        let row_bytes = width
            .checked_mul(3)
            .ok_or_else(|| "BMP row size overflowed this platform".to_string())?;

        let row_stride = row_bytes
            .checked_add(3)
            .ok_or_else(|| "BMP row padding overflowed this platform".to_string())?
            & !3;

        let pixel_bytes = row_stride
            .checked_mul(height)
            .ok_or_else(|| "BMP pixel array size overflowed this platform".to_string())?;

        let pixel_end = pixel_offset
            .checked_add(pixel_bytes)
            .ok_or_else(|| "BMP pixel array offset overflowed this platform".to_string())?;

        if pixel_offset < 54 || pixel_end > bytes.len() {
            return Err("BMP pixel array points outside the file".to_string());
        }

        Ok(Self {
            bytes,
            pixel_offset,
            width,
            height,
            row_stride,
            top_down: signed_height < 0,
        })
    }

    fn pixel_index(&self, x: usize, y: usize) -> usize {
        debug_assert!(x < self.width);
        debug_assert!(y < self.height);

        let file_row = if self.top_down {
            y
        } else {
            self.height - 1 - y
        };

        self.pixel_offset + file_row * self.row_stride + x * 3
    }

    #[cfg(test)]
    pub(crate) fn test_image(width: usize, height: usize) -> Self {
        let row_stride = (width * 3 + 3) & !3;
        let pixel_offset = 54;
        let file_size = pixel_offset + row_stride * height;
        let mut bytes = vec![0_u8; file_size];

        bytes[0..2].copy_from_slice(b"BM");
        bytes[2..6].copy_from_slice(&(file_size as u32).to_le_bytes());
        bytes[10..14].copy_from_slice(&(pixel_offset as u32).to_le_bytes());
        bytes[14..18].copy_from_slice(&40_u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&(width as i32).to_le_bytes());
        bytes[22..26].copy_from_slice(&(height as i32).to_le_bytes());
        bytes[26..28].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&24_u16.to_le_bytes());

        for y in 0..height {
            for x in 0..width {
                let file_row = height - 1 - y;
                let index = pixel_offset + file_row * row_stride + x * 3;
                let base = ((x * 7 + y * 11) % 180 + 32) as u8;

                bytes[index] = base.saturating_add(8);
                bytes[index + 1] = base;
                bytes[index + 2] = base.saturating_sub(8);
            }
        }

        Self::from_bytes(bytes).expect("generated test BMP should be valid")
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let raw = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "BMP header ended unexpectedly".to_string())?;

    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let raw = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "BMP header ended unexpectedly".to_string())?;

    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, String> {
    let raw = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "BMP header ended unexpectedly".to_string())?;

    Ok(i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}
