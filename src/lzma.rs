extern "C" {
    pub fn lzma_create(hunkbytes: u32) -> usize;
    pub fn lzma_destroy(dec: usize);
    pub fn lzma_decompress(
        dec: usize,
        src: *const u8,
        complen: u32,
        dest: *mut u8,
        destlen: u32,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linkage() {
        unsafe {
            let smth = lzma_create(4096);
            assert_ne!(smth, 0);
            lzma_destroy(smth);
        }
    }
}
