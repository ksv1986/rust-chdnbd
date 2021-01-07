pub struct BitReader<'a> {
    data: &'a [u8], // read pointer
    buffer: u32,    // current bit accumulator
    bits: usize,    // number of bits in the accumulator
    offset: usize,  // byte offset within the data
}

fn check(numbits: usize) {
    if numbits >= 32 {
        panic!("number of bits to read must be < 32")
    }
}

impl BitReader<'_> {
    pub fn new<'a>(data: &'a [u8]) -> BitReader {
        BitReader {
            data,
            buffer: 0,
            bits: 0,
            offset: 0,
        }
    }

    pub fn overflow(&self) -> bool {
        self.offset - self.bits / 8 > self.data.len()
    }

    pub fn peek(&mut self, numbits: usize) -> u32 {
        check(numbits);

        if numbits == 0 {
            return 0;
        }

        if numbits > self.bits {
            while self.bits <= 24 {
                if self.offset < self.data.len() {
                    let byte = self.data[self.offset] as u32;
                    self.buffer |= byte << (24 - self.bits);
                }
                self.offset += 1;
                self.bits += 8;
            }
        }

        self.buffer >> (32 - numbits)
    }

    pub fn seek(&mut self, numbits: usize) {
        check(numbits);
        self.buffer <<= numbits;
        self.bits -= numbits;
    }

    pub fn read(&mut self, numbits: usize) -> u32 {
        let val = self.peek(numbits);
        self.seek(numbits);
        val
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reader() {
        let mut bit_reader = BitReader::new(&[0b11110011, 0b01100010]);

        assert_eq!(bit_reader.read(4), 0b1111);
        assert_eq!(bit_reader.read(2), 0b__00);
        assert_eq!(bit_reader.read(4), 0b1101);
        assert_eq!(bit_reader.read(6), 0b100010);
        assert_eq!(bit_reader.overflow(), false);
        assert_eq!(bit_reader.read(31), 0);
        assert_eq!(bit_reader.overflow(), true);
    }
}
