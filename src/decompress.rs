extern crate inflate;

use std::io;
use std::io::Write;

use crate::bitstream::BitReader;
use crate::huffman;
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
    fn decompress(&mut self, src: &[u8], dest: &mut [u8]) -> io::Result<()> {
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

pub struct Inflate {
}

impl Inflate {
    pub fn new() -> Self { Self {} }
}

impl Decompressor for Inflate {
    fn decompress(&mut self, src: &[u8], dest: &mut [u8]) -> io::Result<()> {
        let mut inflate = inflate::InflateWriter::new(dest);
        inflate.write(&src)?;
        Ok(())
    }
}
