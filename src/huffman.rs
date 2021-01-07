use crate::bitstream::BitReader;
use crate::utils::*;
use std::io;

type LookupValue = u16;
type ValueSize = u8;
type NodeIndex = u32;

const NUM_CODES: NodeIndex = 16;
const MAX_BITS: ValueSize = 8;

const fn make_lookup(code: usize, bits: ValueSize) -> LookupValue {
    ((code as LookupValue) << 5) | ((bits as LookupValue) & 0x1f)
}

#[derive(Clone, Copy)]
struct Node {
    bits: ValueSize,    // bits used to encode the node
    numbits: ValueSize, // number of bits needed for this node
}

fn set_num_bits(nodes: &mut [Node], curnode: u32, nodebits: u32) {
    nodes[curnode as usize].numbits = nodebits as u8;
}

pub struct Huffman {
    numcodes: NodeIndex,
    maxbits: ValueSize,
    lookup: [LookupValue; 1 << MAX_BITS],
}

impl Huffman {
    pub fn new() -> Self {
        Huffman {
            numcodes: NUM_CODES,
            maxbits: MAX_BITS,
            lookup: [0; 1 << MAX_BITS],
        }
    }

    pub fn decode_one(&self, stream: &mut BitReader) -> LookupValue {
        // peek ahead to get maxbits worth of data */
        let bits = stream.peek(self.maxbits as usize);
        // look it up, then remove the actual number of bits for this code
        let lookup = self.lookup[bits as usize];
        stream.seek((lookup as usize) & 0x1f);
        // return the value
        return lookup >> 5;
    }

    pub fn import_tree_rle(&mut self, stream: &mut BitReader) -> io::Result<()> {
        let mut nodes = [Node {
            bits: 0,
            numbits: 0,
        }; NUM_CODES as usize];
        self.read_num_bits(stream, &mut nodes)?;
        self.assign_canonical_codes(&mut nodes)?;
        self.build_lookup_table(&nodes);
        Ok(())
    }

    fn read_num_bits(&mut self, stream: &mut BitReader, nodes: &mut [Node]) -> io::Result<()> {
        // bits per entry depends on the maxbits
        let numbits = match self.maxbits {
            0..=7 => 3,
            8..=15 => 4,
            _ => 5,
        };

        // loop until we read numbits for all nodes
        let mut curnode = 0;
        while curnode < self.numcodes {
            let nodebits = stream.read(numbits);
            if nodebits != 1 {
                // a non-one value is just raw
                set_num_bits(nodes, curnode, nodebits);
                curnode += 1;
                continue;
            }

            // a one value is an escape code
            let nodebits = stream.read(numbits);
            if nodebits == 1 {
                // a double 1 is just a single 1
                set_num_bits(nodes, curnode, nodebits);
                curnode += 1;
            } else {
                // otherwise, we need one for value for the repeat count
                let repcount = stream.read(numbits) + 3;
                for _ in 0..repcount {
                    set_num_bits(nodes, curnode, nodebits);
                    curnode += 1;
                }
            }
        }

        /* make sure we ended up with the right number */
        if curnode != self.numcodes {
            return Err(invalid_data("wrong number or huffman codes"));
        }

        if stream.overflow() {
            return Err(invalid_data("rle buffer too small"));
        }

        Ok(())
    }

    fn assign_canonical_codes(&mut self, nodes: &mut [Node]) -> io::Result<()> {
        let mut bithisto = [0; 33];

        // build up a histogram of bit lengths
        for node in nodes.iter() {
            let numbits = node.numbits;
            if numbits > self.maxbits {
                return Err(invalid_data("inconsistent bit lengths"));
            }
            if numbits <= 32 {
                bithisto[numbits as usize] += 1;
            }
        }

        // for each code length, determine the starting code number
        let mut curstart = 0;
        for codelen in (1..32).rev() {
            let nextstart = (curstart + bithisto[codelen]) >> 1;
            if codelen != 1 && nextstart * 2 != (curstart + bithisto[codelen]) {
                return Err(invalid_data("inconsistent starting codes"));
            }
            bithisto[codelen] = curstart;
            curstart = nextstart;
        }

        // now assign canonical codes
        for node in nodes.iter_mut() {
            let numbits = node.numbits as usize;
            if numbits > 0 {
                node.bits = bithisto[numbits];
                bithisto[numbits] += 1;
            }
        }
        Ok(())
    }

    fn build_lookup_table(&mut self, nodes: &[Node]) {
        // iterate over all codes
        for (curcode, node) in nodes.iter().enumerate() {
            let numbits = node.numbits;
            // process all nodes which have non-zero bits */
            if numbits == 0 {
                continue;
            }
            // set up the entry
            let value = make_lookup(curcode, numbits);
            // fill all matching entries
            let shift = self.maxbits - numbits;
            let begin = (node.bits as usize) << shift;
            let end = (node.bits as usize + 1) << shift;
            for e in self.lookup[begin..end].iter_mut() {
                *e = value;
            }
        }
    }
}
