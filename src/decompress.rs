extern crate claxon;
extern crate inflate;

use claxon::frame::FrameReader;
use claxon::input::BufferedReader;
use std::io;
use std::io::{Cursor, Write};

use crate::bitstream::BitReader;
use crate::huffman;
use crate::lzma::*;
use crate::utils::*;

pub trait Decompressor {
    fn decompress(&mut self, src: &[u8], dest: &mut [u8]) -> io::Result<()>;
}

pub struct Unknown {
    tag: u32,
}

impl Unknown {
    pub fn new(tag: u32) -> Self {
        Self { tag }
    }
}

impl Decompressor for Unknown {
    fn decompress(&mut self, _src: &[u8], _dest: &mut [u8]) -> io::Result<()> {
        Err(invalid_data_owned(format!(
            "codec {:08x} not implemented",
            self.tag
        )))
    }
}

pub struct Huffman {
    huffman: huffman::Huffman,
}

impl Huffman {
    pub fn new() -> Self {
        Self {
            huffman: huffman::Huffman::new(256, 16),
        }
    }
}

impl Decompressor for Huffman {
    fn decompress(&mut self, src: &[u8], dest: &mut [u8]) -> io::Result<()> {
        let mut stream = BitReader::new(src);
        self.huffman.import_tree_huffman(&mut stream)?;
        for byte in dest.iter_mut() {
            *byte = self.huffman.decode_one(&mut stream) as u8;
        }
        match stream.overflow() {
            false => Ok(()),
            true => Err(invalid_data("compressed hunk is too small")),
        }
    }
}

pub struct Inflate {}

impl Inflate {
    pub fn new() -> Self {
        Self {}
    }
}

impl Decompressor for Inflate {
    fn decompress(&mut self, src: &[u8], dest: &mut [u8]) -> io::Result<()> {
        let mut inflate = inflate::InflateWriter::new(dest);
        inflate.write(&src)?;
        Ok(())
    }
}

pub struct Lzma {
    handle: usize,
}

impl Lzma {
    pub fn new(hunkbytes: u32) -> io::Result<Self> {
        let handle = unsafe { lzma_create(hunkbytes) };
        match handle {
            0 => Err(invalid_data("failed to create lzma decoder")),
            _ => Ok(Self { handle }),
        }
    }
}

impl Drop for Lzma {
    fn drop(&mut self) {
        unsafe { lzma_destroy(self.handle) };
    }
}

impl Decompressor for Lzma {
    fn decompress(&mut self, src: &[u8], dest: &mut [u8]) -> io::Result<()> {
        let error = unsafe {
            let srclen = src.len() as u32;
            let psrc = src.as_ptr();
            let dstlen = dest.len() as u32;
            let pdst = dest.as_mut_ptr();
            lzma_decompress(self.handle, psrc, srclen, pdst, dstlen)
        };
        match error {
            0 => Ok(()),
            _ => Err(invalid_data("lzma decompression failed")),
        }
    }
}

pub struct Flac {}

impl Flac {
    pub fn new() -> Self {
        Self {}
    }
}

impl Decompressor for Flac {
    fn decompress(&mut self, src: &[u8], dest: &mut [u8]) -> io::Result<()> {
        let write_endian = match src[0] {
            b'L' => write_le16,
            b'B' => write_be16,
            x => {
                return Err(invalid_data_owned(format!(
                    "invalid flac endianness {:x}",
                    x
                )))
            }
        };
        let input = Cursor::new(&src[1..]);
        let buffered_reader = BufferedReader::new(input);
        let mut frame_reader = FrameReader::new(buffered_reader);
        let frame_size = 4; // 16bit stereo
        let num_frames = dest.len() / frame_size;
        let buffer = vec![0; num_frames];
        let result = frame_reader
            .read_next_or_eof(buffer)
            .map_err(|_| invalid_data("failed to decode frame"))?;
        let block = result.ok_or(invalid_data("flac data is too short"))?;
        if block.duration() != num_frames as u32 {
            return Err(invalid_data(
                "decoded flac duration doesn't match number of frames in hunk",
            ));
        }
        if block.channels() != 2 {
            return Err(invalid_data_owned(format!(
                "expected stereo, but got {} channel samples",
                block.channels()
            )));
        }
        for (i, (sl, sr)) in block.stereo_samples().enumerate() {
            write_endian(&mut dest[i * frame_size + 0..i * frame_size + 2], sl as u16);
            write_endian(&mut dest[i * frame_size + 2..i * frame_size + 4], sr as u16);
        }
        Ok(())
    }
}
