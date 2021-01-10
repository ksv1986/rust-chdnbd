extern crate inflate;

use std::io;
use std::io::Write;

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
