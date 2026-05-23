use crate::error::{BufrError, Result};

#[derive(Debug, Clone)]
pub struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    pub fn read_u64(&mut self, width: u16) -> Result<u64> {
        if width == 0 {
            return Ok(0);
        }
        if width > 64 {
            return Err(BufrError::Table(format!(
                "cannot read {width} bits into u64"
            )));
        }
        if self.bit_pos + width as usize > self.data.len() * 8 {
            return Err(BufrError::Table(format!(
                "not enough bits while reading section 4 at bit {} for width {} ({} bits available)",
                self.bit_pos,
                width,
                self.data.len() * 8
            )));
        }

        let mut value = 0u64;
        for _ in 0..width {
            let byte = self.data[self.bit_pos / 8];
            let shift = 7 - (self.bit_pos % 8);
            let bit = (byte >> shift) & 1;
            value = (value << 1) | bit as u64;
            self.bit_pos += 1;
        }
        Ok(value)
    }

    pub fn read_bytes(&mut self, width: u16) -> Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(width.div_ceil(8) as usize);
        let mut remaining = width;
        while remaining >= 8 {
            bytes.push(self.read_u64(8)? as u8);
            remaining -= 8;
        }
        if remaining > 0 {
            bytes.push((self.read_u64(remaining)? << (8 - remaining)) as u8);
        }
        Ok(bytes)
    }
}
