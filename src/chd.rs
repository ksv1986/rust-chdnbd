use std::convert::TryFrom;
use std::io;
use std::io::{Read, Seek, SeekFrom};

extern crate crc16;
use crate::bitstream::BitReader;
use crate::huffman::Huffman;
use crate::utils::*;

#[derive(Debug, PartialEq)]
pub enum Version {
    V4 = 4,
    V5 = 5,
}

/* codec #0
 * these types are live when running */
const COMPRESSION_TYPE_0: u8 = 0;
/* codec #1 */
const COMPRESSION_TYPE_1: u8 = 1;
/* codec #2 */
const COMPRESSION_TYPE_2: u8 = 2;
/* codec #3 */
const COMPRESSION_TYPE_3: u8 = 3;
/* no compression; implicit length = hunkbytes */
const COMPRESSION_NONE: u8 = 4;
/* same as another block in this chd */
const COMPRESSION_SELF: u8 = 5;
/* same as a hunk's worth of units in the parent chd */
const COMPRESSION_PARENT: u8 = 6;

/* start of small RLE run (4-bit length)
 * these additional pseudo-types are used for compressed encodings: */
const COMPRESSION_RLE_SMALL: u8 = 7;
/* start of large RLE run (8-bit length) */
const COMPRESSION_RLE_LARGE: u8 = 8;
/* same as the last COMPRESSION_SELF block */
const COMPRESSION_SELF_0: u8 = 9;
/* same as the last COMPRESSION_SELF block + 1 */
const COMPRESSION_SELF_1: u8 = 10;
/* same block in the parent */
const COMPRESSION_PARENT_SELF: u8 = 11;
/* same as the last COMPRESSION_PARENT block */
const COMPRESSION_PARENT_0: u8 = 12;
/* same as the last COMPRESSION_PARENT block + 1 */
const COMPRESSION_PARENT_1: u8 = 13;

impl Default for Version {
    fn default() -> Self {
        Version::V5
    }
}

fn read_be16(data: &[u8]) -> u16 {
    assert_eq!(data.len(), 2);
    (data[0] as u16) << 8 | data[1] as u16
}

fn read_be32(data: &[u8]) -> u32 {
    assert_eq!(data.len(), 4);
    (data[0] as u32) << 24 | (data[1] as u32) << 16 | (data[2] as u32) << 8 | data[3] as u32
}

fn read_be48(data: &[u8]) -> u64 {
    assert_eq!(data.len(), 6);
    (data[0] as u64) << 40
        | (data[1] as u64) << 32
        | (data[2] as u64) << 24
        | (data[3] as u64) << 16
        | (data[4] as u64) << 8
        | data[5] as u64
}

fn read_be64(data: &[u8]) -> u64 {
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

fn read_bit_length(data: &[u8], offs: usize) -> io::Result<u8> {
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

fn write_be16(data: &mut [u8], val: u16) {
    write_be(data, 2, val)
}

fn write_be24(data: &mut [u8], val: u32) {
    write_be(data, 3, val)
}

fn write_be48(data: &mut [u8], val: u64) {
    write_be(data, 6, val)
}

pub struct Chd<T: Read + Seek> {
    io: T,
    initialized: bool,
    vers: Version,
    pos: i64,
    size: i64,

    // internal state
    compressors: [u32; 4],
    hunkbytes: u32,
    hunkcount: u32,
    unitbytes: u32,
    map: Vec<u8>, // decompressed hunk map
}

impl<T: Read + Seek> Chd<T> {
    pub fn new(io: T) -> Chd<T> {
        Chd {
            io: io,
            initialized: false,
            vers: Version::default(),
            pos: 0,
            size: 0,
            compressors: [0, 0, 0, 0],
            hunkbytes: 0,
            hunkcount: 0,
            unitbytes: 0,
            map: Vec::new(),
        }
    }

    pub fn len(&self) -> u64 {
        self.size as u64
    }

    pub fn version(&self) -> &Version {
        &self.vers
    }

    fn sanity_check(&self) -> io::Result<()> {
        if self.hunkbytes < 1 || self.hunkbytes > 512 * 1024 {
            return Err(invalid_data("wrong size of hunk"));
        }
        Ok(())
    }

    fn decompress_v5_map(&mut self, mapoffset: u64) -> io::Result<()> {
        const MAP_ENTRY_SIZE: usize = 12;
        let hunkcount = self.hunkcount as usize;
        if self.compressors[0] == 0 {
            // uncompressed
            self.map = vec![0; MAP_ENTRY_SIZE * hunkcount];
            return self.io.read_exact(self.map.as_mut_slice());
        }

        self.io.seek(SeekFrom::Start(mapoffset))?;

        let mut maphdr = [0u8; 16];
        self.io.read_exact(&mut maphdr)?;

        let maplength = read_be32(&maphdr[0..4]);
        let lengthbits = read_bit_length(&maphdr, 12)?;
        let hunkbits = read_bit_length(&maphdr, 13)?;
        let parentbits = read_bit_length(&maphdr, 14)?;

        let mut comprmap = vec![0u8; maplength as usize];
        self.io.read_exact(comprmap.as_mut_slice())?;

        let mut bits = BitReader::new(&comprmap);
        let mut huffman = Huffman::new();
        huffman.import_tree_rle(&mut bits)?;

        let hunkcount = self.hunkcount as usize;
        self.map = vec![0u8; MAP_ENTRY_SIZE * hunkcount];

        // first decode the compression types
        let mut repcount = 0;
        let mut lastcomp = 0;
        for hunknum in 0..hunkcount {
            if repcount > 0 {
                repcount -= 1;
            } else {
                match huffman.decode_one(&mut bits) as u8 {
                    COMPRESSION_RLE_SMALL => {
                        repcount = 2 + huffman.decode_one(&mut bits);
                    }
                    COMPRESSION_RLE_LARGE => {
                        repcount = 2 + 16 + (huffman.decode_one(&mut bits) << 4);
                        repcount += huffman.decode_one(&mut bits);
                    }
                    val => {
                        lastcomp = val;
                    }
                }
            }
            let compression = &mut self.map[hunknum * MAP_ENTRY_SIZE];
            *compression = lastcomp;
        }

        // then iterate through the hunks and extract the needed data
        let mut curoffset = read_be48(&maphdr[4..10]);
        let mut lastself = 0;
        let mut lastparent = 0;
        for hunknum in 0..hunkcount {
            let mut offset = curoffset;
            let mut length = 0;
            let mut crc = 0;
            let mapentry = &mut self.map[hunknum * MAP_ENTRY_SIZE..(hunknum + 1) * MAP_ENTRY_SIZE];
            let compression = &mut mapentry[0];
            match *compression {
                // base types
                COMPRESSION_TYPE_0 | COMPRESSION_TYPE_1 | COMPRESSION_TYPE_2
                | COMPRESSION_TYPE_3 => {
                    length = bits.read(lengthbits as usize);
                    curoffset += length as u64;
                    crc = bits.read(16) as u16;
                }
                COMPRESSION_NONE => {
                    length = self.hunkbytes;
                    curoffset += length as u64;
                    crc = bits.read(16) as u16;
                }
                COMPRESSION_SELF => {
                    offset = bits.read(hunkbits as usize) as u64;
                    lastself = offset;
                }
                COMPRESSION_PARENT => {
                    offset = bits.read(parentbits as usize) as u64;
                    lastparent = offset;
                }
                // pseudo-types; convert into base types
                COMPRESSION_SELF_0 | COMPRESSION_SELF_1 => {
                    lastself += (*compression - COMPRESSION_SELF_0) as u64;
                    offset = lastself;
                    *compression = COMPRESSION_SELF;
                }
                COMPRESSION_PARENT_SELF => {
                    lastparent =
                        ((hunknum as u64) * (self.hunkbytes as u64)) / (self.unitbytes as u64);
                    offset = lastparent;
                    *compression = COMPRESSION_SELF;
                }
                COMPRESSION_PARENT_0 | COMPRESSION_PARENT_1 => {
                    if *compression == COMPRESSION_PARENT_1 {
                        lastparent += (self.hunkbytes / self.unitbytes) as u64;
                    }
                    offset = lastparent;
                    *compression = COMPRESSION_PARENT;
                }
                _ => return Err(invalid_data("unknown hunk compression type")),
            }
            if length >= 0x1_00_00_00 {
                // 24 bits
                return Err(invalid_data("hunk length is too big"));
            }
            if offset >= 0x1_00_00_00_00_00_00 {
                // 48 bits
                return Err(invalid_data("hunk offset is too big"));
            }
            write_be24(&mut mapentry[1..4], length);
            write_be48(&mut mapentry[4..10], offset);
            write_be16(&mut mapentry[10..12], crc);
        }
        let crc = read_be16(&maphdr[10..12]);
        let calc = crc16::State::<crc16::CCITT_FALSE>::calculate(&self.map);
        if crc != calc {
            return Err(invalid_data("map decompression failed"));
        }
        Ok(())
    }

    fn read_header_v5(&mut self, data: &[u8]) -> io::Result<()> {
        self.vers = Version::V5;
        self.size = read_be64(&data[32..40]) as i64;
        self.compressors[0] = read_be32(&data[16..20]);
        self.compressors[1] = read_be32(&data[20..24]);
        self.compressors[2] = read_be32(&data[24..28]);
        self.compressors[3] = read_be32(&data[28..32]);
        self.hunkbytes = read_be32(&data[56..60]);
        let length = read_be32(&data[8..12]);
        if length != 124 {
            return Err(invalid_data("invalid v5 header length"));
        }
        self.sanity_check()?;
        let hunkcount = self.size as u64 / self.hunkbytes as u64;
        self.hunkcount = u32::try_from(hunkcount).map_err(|_| invalid_data("wrong hunk count"))?;

        let mapoffset = read_be64(&data[40..48]);
        self.decompress_v5_map(mapoffset)?;

        self.initialized = true;
        Ok(())
    }

    fn read_header_v4(&mut self, data: &[u8]) -> io::Result<()> {
        self.vers = Version::V4;
        self.size = read_be64(&data[28..36]) as i64;
        self.compressors[0] = read_be32(&data[20..24]);
        // TODO
        self.initialized = true;
        Ok(())
    }

    pub fn read_header(&mut self) -> io::Result<()> {
        if self.initialized {
            return Ok(());
        }

        let mut data = [0u8; 124];

        self.io.seek(SeekFrom::Start(0))?;
        self.io.read_exact(&mut data)?;

        if &data[0..8] != b"MComprHD" {
            return Err(invalid_data("invalid magic"));
        }

        match read_be32(&data[12..16]) {
            5 => self.read_header_v5(&data),
            4 => self.read_header_v4(&data),
            _ => Err(invalid_data("unsupported version")),
        }
    }
}

impl<T: Read + Seek> Read for Chd<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.read_header()?;
        Err(io::Error::new(io::ErrorKind::Other, "not implemented"))
    }
}

impl<T: Read + Seek> Seek for Chd<T> {
    fn seek(&mut self, sf: SeekFrom) -> io::Result<u64> {
        self.read_header()?;
        match sf {
            SeekFrom::Start(x) => {
                self.pos = x as i64;
            }
            SeekFrom::Current(x) => {
                if let Some(xx) = self.pos.checked_add(x) {
                    self.pos = xx;
                } else {
                    return Err(invalid_data("Invalid seek"));
                }
            }
            SeekFrom::End(x) => {
                if let Some(xx) = self.size.checked_add(x) {
                    self.pos = xx;
                } else {
                    return Err(invalid_data("Invalid seek"));
                }
            }
        }
        Ok(self.pos as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        let mut c = {
            //let d = std::fs::File::open("raycris.chd").unwrap();
            let d = io::Cursor::new(include_bytes!("../raycris.chd"));
            Chd::new(d)
        };
        c.read_header().unwrap();
        assert_eq!(c.version(), &Version::V5);
        assert_eq!(c.len(), 40_960_000);
        assert_eq!(c.seek(SeekFrom::Current(0)).unwrap(), 0);
        assert_eq!(c.seek(SeekFrom::End(0)).unwrap(), c.len());
    }
}
