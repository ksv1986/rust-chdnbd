extern crate crc16;

use std::convert::TryFrom;
use std::io;
use std::io::{Read, Seek, SeekFrom};

use crate::bitstream::BitReader;
use crate::decompress;
use crate::decompress::Decompressor;
use crate::huffman::Huffman;
use crate::utils::*;

const V5: u32 = 5;

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

const COMPRESSION_NUM: usize = 7;

const fn make_tag(data: [char; 4]) -> u32 {
    (data[0] as u32) << 24 | (data[1] as u32) << 16 | (data[2] as u32) << 8 | data[3] as u32
}

const CHD_CODEC_ZLIB: u32 = make_tag(['z', 'l', 'i', 'b']);
const CHD_CODEC_HUFF: u32 = make_tag(['h', 'u', 'f', 'f']);
const CHD_CODEC_LZMA: u32 = make_tag(['l', 'z', 'm', 'a']);
const CHD_CODEC_FLAC: u32 = make_tag(['f', 'l', 'a', 'c']);
// general codecs with CD frontend
const CHD_CODEC_CD_ZLIB: u32 = make_tag(['c', 'd', 'z', 'l']);
const CHD_CODEC_CD_LZMA: u32 = make_tag(['c', 'd', 'l', 'z']);
const CHD_CODEC_CD_FLAC: u32 = make_tag(['c', 'd', 'f', 'l']);

fn crc16(data: &[u8]) -> u16 {
    crc16::State::<crc16::CCITT_FALSE>::calculate(data)
}

#[derive(Default)]
struct Header {
    version: u32,
    length: u32,
    size: u64,
    mapoffset: u64,
    compressors: [u32; 4],
    hunkbytes: u32,
    hunkcount: u32,
    unitbytes: u32,
}

impl Header {
    fn read<T: Read + Seek>(io: &mut T) -> io::Result<Self> {
        let mut data = [0u8; 124];
        io.read_at(0, &mut data)?;

        if &data[0..8] != b"MComprHD" {
            return Err(invalid_data("invalid magic"));
        }

        let mut header = Header::default();
        header.version = read_be32(&data[12..16]);
        match header.version {
            V5 => header.read_header_v5(&data)?,
            _ => return Err(invalid_data("unsupported version")),
        }
        Ok(header)
    }

    fn read_header_v5(&mut self, data: &[u8]) -> io::Result<()> {
        self.length = read_be32(&data[8..12]);
        if self.length != 124 {
            return Err(invalid_data("invalid v5 header length"));
        }
        self.size = read_be64(&data[32..40]);
        self.compressors[0] = read_be32(&data[16..20]);
        self.compressors[1] = read_be32(&data[20..24]);
        self.compressors[2] = read_be32(&data[24..28]);
        self.compressors[3] = read_be32(&data[28..32]);
        self.hunkbytes = read_be32(&data[56..60]);
        self.unitbytes = read_be32(&data[60..64]);
        if self.hunkbytes < 1 || self.hunkbytes > 512 * 1024 {
            return Err(invalid_data("wrong size of hunk"));
        }
        if self.unitbytes < 1
            || self.hunkbytes < self.unitbytes
            || self.hunkbytes % self.unitbytes > 0
        {
            return Err(invalid_data("wrong size of unit"));
        }
        let hunkcount = self.size as u64 / self.hunkbytes as u64;
        self.hunkcount = u32::try_from(hunkcount).map_err(|_| invalid_data("wrong hunk count"))?;
        self.mapoffset = read_be64(&data[40..48]);
        Ok(())
    }
}

trait Map {
    fn compression(&self, hunknum: usize) -> u8;
    fn validate(&self, hunknum: usize, buf: &[u8]) -> bool;
    fn locate(&self, hunknum: usize) -> (u8, u64, u32);
}

struct Map5 {
    map: Vec<u8>, // decompressed hunk map
}

impl Map for Map5 {
    fn compression(&self, hunknum: usize) -> u8 {
        self.map[Map5::offset(hunknum)]
    }

    fn validate(&self, hunknum: usize, buf: &[u8]) -> bool {
        let offs = Map5::offset(hunknum);
        let crc = read_be16(&self.map[offs + 10..offs + 12]);
        let calc = crc16(buf);
        crc == calc
    }

    fn locate(&self, hunknum: usize) -> (u8, u64, u32) {
        let offs = Map5::offset(hunknum);
        let compression = self.map[offs];
        let offset = read_be48(&self.map[offs + 4..offs + 10]);
        let length = read_be24(&self.map[offs + 1..offs + 4]);
        (compression, offset, length)
    }
}

impl Map5 {
    fn read<T: Read + Seek>(io: &mut T, header: &Header) -> io::Result<Self> {
        let hunkcount = header.hunkcount as usize;
        let mut map5 = Self {
            map: vec![0; Map5::offset(hunkcount)],
        };
        if header.compressors[0] == 0 {
            // uncompressed
            io.read_at(header.mapoffset, map5.map.as_mut_slice())?;
        } else {
            let mut maphdr = [0u8; 16];
            io.read_at(header.mapoffset, &mut maphdr)?;

            let maplength = read_be32(&maphdr[0..4]);
            let mut comprmap = vec![0u8; maplength as usize];
            io.read_exact(comprmap.as_mut_slice())?;

            map5.decompress(header, &maphdr, &comprmap)?;
        }
        Ok(map5)
    }

    fn decompress(&mut self, header: &Header, maphdr: &[u8], comprmap: &[u8]) -> io::Result<()> {
        let hunkcount = header.hunkcount as usize;
        let hunkbytes = header.hunkbytes;
        let unitbytes = header.unitbytes;

        let lengthbits = read_bit_length(&maphdr, 12)?;
        let hunkbits = read_bit_length(&maphdr, 13)?;
        let parentbits = read_bit_length(&maphdr, 14)?;

        let mut bits = BitReader::new(&comprmap);
        let mut huffman = Huffman::new(16, 8);
        huffman.import_tree_rle(&mut bits)?;

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
            let compression = &mut self.map[Map5::offset(hunknum)];
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
            let mapentry = &mut self.map[Map5::offset(hunknum)..Map5::offset(hunknum + 1)];
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
                    length = hunkbytes;
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
                    lastparent = ((hunknum as u64) * (hunkbytes as u64)) / (unitbytes as u64);
                    offset = lastparent;
                    *compression = COMPRESSION_SELF;
                }
                COMPRESSION_PARENT_0 | COMPRESSION_PARENT_1 => {
                    if *compression == COMPRESSION_PARENT_1 {
                        lastparent += (hunkbytes / unitbytes) as u64;
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
        let calc = crc16(&self.map);
        if crc != calc {
            return Err(invalid_data("map decompression failed"));
        }
        Ok(())
    }

    const fn offset(hunknum: usize) -> usize {
        12 * hunknum
    }
}

pub struct Chd<T: Read + Seek> {
    pos: i64,
    io: T,
    header: Header,
    map: Box<dyn Map>,
    decompress: [Option<Box<dyn Decompressor>>; 4],
}

impl<T: Read + Seek> Chd<T> {
    pub fn open(mut io: T) -> io::Result<Chd<T>> {
        let header = Header::read(&mut io)?;
        let map = match header.version {
            V5 => Map5::read(&mut io, &header)?,
            _ => return Err(invalid_data("unsupported map version")),
        };
        let mut chd = Self {
            pos: 0,
            io,
            header,
            map: Box::new(map),
            decompress: [None, None, None, None],
        };
        for (i, d) in chd.decompress.iter_mut().enumerate() {
            match chd.header.compressors[i] {
                0 => (),
                CHD_CODEC_HUFF => *d = Some(Box::new(decompress::Huffman::new())),
                x => *d = Some(Box::new(decompress::Unknown::new(x))),
            }
        }
        Ok(chd)
    }

    pub fn len(&self) -> u64 {
        self.header.size
    }

    pub fn hunk_size(&self) -> u32 {
        self.header.hunkbytes
    }

    pub fn hunk_count(&self) -> u32 {
        self.header.hunkcount
    }

    pub fn version(&self) -> u32 {
        self.header.version
    }

    pub fn compression_name(&self, compr: u8) -> &'static str {
        match compr {
            COMPRESSION_TYPE_0 => self.codec_name(0).unwrap_or("Codec #0"),
            COMPRESSION_TYPE_1 => self.codec_name(1).unwrap_or("Codec #1"),
            COMPRESSION_TYPE_2 => self.codec_name(2).unwrap_or("Codec #2"),
            COMPRESSION_TYPE_3 => self.codec_name(3).unwrap_or("Codec #3"),
            COMPRESSION_NONE => "Uncompressed",
            COMPRESSION_SELF => "Self",
            COMPRESSION_PARENT => "Parent",
            _ => "Invalid",
        }
    }

    pub fn codec_name(&self, i: usize) -> Option<&'static str> {
        if i == 0 && self.header.compressors[0] == 0 {
            return Some("None");
        }
        if i < 4 {
            match self.header.compressors[i] {
                0 => None,
                CHD_CODEC_HUFF => Some("Huffman"),
                CHD_CODEC_ZLIB => Some("zlib"),
                CHD_CODEC_LZMA => Some("LZMA"),
                CHD_CODEC_FLAC => Some("FLAC"),
                CHD_CODEC_CD_ZLIB => Some("CD zlib"),
                CHD_CODEC_CD_LZMA => Some("CD LZMA"),
                CHD_CODEC_CD_FLAC => Some("CD FLAC"),
                _ => Some("Unknown"),
            }
        } else {
            None
        }
    }

    pub fn compression_distribution(&self) -> [u32; COMPRESSION_NUM] {
        let mut distr = [0; COMPRESSION_NUM];
        if self.header.compressors[0] == 0 {
            distr[COMPRESSION_NONE as usize] = self.header.hunkcount;
        } else {
            for hunk in 0..self.header.hunkcount as usize {
                let map = &self.map;
                let compressor = map.compression(hunk) as usize;
                if compressor < COMPRESSION_NUM {
                    distr[compressor] += 1;
                }
            }
        }
        distr
    }

    fn decompress_hunk(
        &mut self,
        offset: u64,
        length: u32,
        index: usize,
        buf: &mut [u8],
    ) -> io::Result<()> {
        if self.decompress[index].is_some() {
            let mut compbuf = vec![0; length as usize];
            self.io.read_at(offset, compbuf.as_mut_slice())?;
            self.decompress[index]
                .as_deref_mut()
                .unwrap()
                .decompress(&compbuf, buf)
        } else {
            Err(invalid_data("no decompressor"))
        }
    }

    fn read_hunk(&mut self, hunknum: usize, buf: &mut [u8]) -> io::Result<()> {
        let (compression, offset, length) = self.map.locate(hunknum);
        match compression {
            COMPRESSION_NONE => self.io.read_at(offset, buf),
            COMPRESSION_SELF => self.read_hunk(offset as usize, buf),
            COMPRESSION_TYPE_0 | COMPRESSION_TYPE_1 | COMPRESSION_TYPE_2 | COMPRESSION_TYPE_3 => {
                self.decompress_hunk(
                    offset,
                    length,
                    (compression - COMPRESSION_TYPE_0) as usize,
                    buf,
                )
            }
            x => Err(invalid_data_owned(format!("unsupported compression {}", x))),
        }
    }

    fn validate_hunk(&mut self, hunknum: usize) -> io::Result<()> {
        let mut buf = vec![0; self.header.hunkbytes as usize];
        self.read_hunk(hunknum, buf.as_mut_slice())?;
        match self.map.validate(hunknum, &buf) {
            true => Ok(()),
            false => Err(invalid_data("hunk validation failed")),
        }
    }
}

impl<T: Read + Seek> Read for Chd<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "not implemented"))
    }
}

impl<T: Read + Seek> Seek for Chd<T> {
    fn seek(&mut self, sf: SeekFrom) -> io::Result<u64> {
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
                let size = self.header.size as i64;
                if let Some(xx) = size.checked_add(x) {
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
            Chd::open(d).unwrap()
        };
        assert_eq!(c.version(), V5);
        assert_eq!(c.len(), 40_960_000);
        assert_eq!(c.seek(SeekFrom::Current(0)).unwrap(), 0);
        assert_eq!(c.seek(SeekFrom::End(0)).unwrap(), c.len());
        let hunks = [
            // 9,  // self
            322,  // huffman
            3999, // uncompressed
        ];
        for i in hunks.iter() {
            c.validate_hunk(*i).unwrap();
        }
    }
}
