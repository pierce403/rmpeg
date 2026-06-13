use crate::{Result, RmpegError};

#[derive(Debug, Clone)]
pub struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            return Err(RmpegError::UnexpectedEof {
                needed: pos,
                remaining: self.data.len(),
            });
        }
        self.pos = pos;
        Ok(())
    }

    pub fn skip(&mut self, len: usize) -> Result<()> {
        let next = self
            .pos
            .checked_add(len)
            .ok_or_else(|| RmpegError::InvalidData("reader position overflow".to_string()))?;
        self.seek(next)
    }

    pub fn read_bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        if self.remaining() < len {
            return Err(RmpegError::UnexpectedEof {
                needed: len,
                remaining: self.remaining(),
            });
        }
        let start = self.pos;
        self.pos += len;
        Ok(&self.data[start..self.pos])
    }

    pub fn read_u16_le(&mut self) -> Result<u16> {
        let bytes = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub fn read_u32_le(&mut self) -> Result<u32> {
        let bytes = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub fn read_fourcc(&mut self) -> Result<[u8; 4]> {
        let bytes = self.read_bytes(4)?;
        Ok([bytes[0], bytes[1], bytes[2], bytes[3]])
    }
}
