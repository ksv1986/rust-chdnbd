use std::convert::TryInto;
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

pub struct Chd<T: Read + Seek> {
    io: T,
    initialized: bool,
    vers: Version,
    pos: i64,
    size: i64,
}

impl<T: Read + Seek> Chd<T> {
    pub fn new(io: T) -> Chd<T> {
        Chd {
            io: io,
            initialized: false,
            vers: Version::default(),
            pos: 0,
            size: 0,
        }
    }

    pub fn len(&self) -> u64 {
        self.size as u64
    }

    pub fn version(&self) -> &Version {
        &self.vers
    }

    fn read_header_v5(&mut self, data: &[u8]) -> io::Result<()> {
        self.vers = Version::V5;
        self.size = i64::from_be_bytes(data[32..40].try_into().unwrap());

        self.initialized = true;
        Ok(())
    }

    fn read_header_v4(&mut self, data: &[u8]) -> io::Result<()> {
        self.vers = Version::V4;
        self.size = i64::from_be_bytes(data[28..36].try_into().unwrap());

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

        match u32::from_be_bytes(data[12..16].try_into().unwrap()) {
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
