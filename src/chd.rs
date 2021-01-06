use std::convert::TryFrom;
use std::io;
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, PartialEq)]
pub enum Version {
    V4 = 4,
    V5 = 5,
}

impl Default for Version {
    fn default() -> Self {
        Version::V5
    }
}

fn invalid_data(msg: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}

fn read_be32(data: &[u8]) -> u32 {
    assert_eq!(data.len(), 4);
    (data[0] as u32) << 24 | (data[1] as u32) << 16 | (data[2] as u32) << 8 | data[3] as u32
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
