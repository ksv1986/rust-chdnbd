use std::io;
use std::io::{Read, Seek, SeekFrom};

pub fn read_be16(data: &[u8]) -> u16 {
    assert_eq!(data.len(), 2);
    (data[0] as u16) << 8 | data[1] as u16
}

pub fn read_be24(data: &[u8]) -> u32 {
    assert_eq!(data.len(), 3);
    (data[0] as u32) << 16 | (data[1] as u32) << 8 | data[2] as u32
}

pub fn read_be32(data: &[u8]) -> u32 {
    assert_eq!(data.len(), 4);
    (data[0] as u32) << 24 | (data[1] as u32) << 16 | (data[2] as u32) << 8 | data[3] as u32
}

pub fn read_be48(data: &[u8]) -> u64 {
    assert_eq!(data.len(), 6);
    (data[0] as u64) << 40
        | (data[1] as u64) << 32
        | (data[2] as u64) << 24
        | (data[3] as u64) << 16
        | (data[4] as u64) << 8
        | data[5] as u64
}

pub fn read_be64(data: &[u8]) -> u64 {
    assert_eq!(data.len(), 8);
    (data[0] as u64) << 56
        | (data[1] as u64) << 48
        | (data[2] as u64) << 40
        | (data[3] as u64) << 32
        | (data[4] as u64) << 24
        | (data[5] as u64) << 16
        | (data[6] as u64) << 8
        | data[7] as u64
}

pub fn read_bit_length(data: &[u8], offs: usize) -> io::Result<u8> {
    match data[offs] {
        32..=u8::MAX => Err(invalid_data("bit length is too big")),
        val => Ok(val),
    }
}

trait WriteBe {
    fn first_byte(&self) -> u8;
    fn shr_byte(&mut self);
}

fn write_be<T: WriteBe>(data: &mut [u8], len: usize, val: T) {
    let mut v = val;
    for offs in (0..len).rev() {
        data[offs] = v.first_byte();
        v.shr_byte();
    }
}

macro_rules! write_be_impl {
    ( $x:ty ) => {
        impl WriteBe for $x {
            fn first_byte(&self) -> u8 {
                (*self & 0xff) as u8
            }
            fn shr_byte(&mut self) {
                *self >>= 8;
            }
        }
    };
}

write_be_impl!(u16);
write_be_impl!(u32);
write_be_impl!(u64);

pub fn write_be16(data: &mut [u8], val: u16) {
    write_be(data, 2, val)
}

pub fn write_be24(data: &mut [u8], val: u32) {
    write_be(data, 3, val)
}

pub fn write_be48(data: &mut [u8], val: u64) {
    write_be(data, 6, val)
}

pub trait ReadAt {
    fn read_at(&mut self, offset: u64, data: &mut [u8]) -> io::Result<()>;
}

impl<T: Read + Seek> ReadAt for T {
    fn read_at(&mut self, offset: u64, data: &mut [u8]) -> io::Result<()> {
        self.seek(SeekFrom::Start(offset))?;
        self.read_exact(data)
    }
}

pub fn invalid_data(msg: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

pub fn invalid_data_owned(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}
