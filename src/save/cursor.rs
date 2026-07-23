use anyhow::{bail, Result};

/// Bounds-checked little-endian reader over a byte slice.
pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

macro_rules! read_num {
    ($name:ident, $ty:ty) => {
        pub fn $name(&mut self) -> Result<$ty> {
            const N: usize = size_of::<$ty>();
            let b = self.take(N)?;
            Ok(<$ty>::from_le_bytes(b.try_into().unwrap()))
        }
    };
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Cursor<'a> {
        Cursor { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn seek(&mut self, pos: usize) -> Result<()> {
        if pos > self.data.len() {
            bail!("seek to {pos:#x} beyond end {:#x}", self.data.len());
        }
        self.pos = pos;
        Ok(())
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            bail!(
                "read of {n} bytes at {:#x} overruns end {:#x}",
                self.pos,
                self.data.len()
            );
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn peek(&self, n: usize) -> &'a [u8] {
        &self.data[self.pos..(self.pos + n).min(self.data.len())]
    }

    read_num!(read_u8, u8);
    read_num!(read_i8, i8);
    read_num!(read_u16, u16);
    read_num!(read_i16, i16);
    read_num!(read_u32, u32);
    read_num!(read_i32, i32);
    read_num!(read_u64, u64);
    read_num!(read_i64, i64);
    read_num!(read_f32, f32);
    read_num!(read_f64, f64);

    pub fn read_str(&mut self, n: usize) -> Result<String> {
        let b = self.take(n)?;
        Ok(b.iter().map(|&c| c as char).collect())
    }
}
