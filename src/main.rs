extern crate nbd;

use std::fs::File;
use std::io;
use std::io::{Read, Result, Seek, Write};
use std::net::{TcpListener, TcpStream};

use nbd::server::{handshake, transmission, Export};

mod bitstream;
mod chd;
mod huffman;
mod utils;

use chd::Chd;

// Sink any writes to Chd. Only needed to satisfy nbd::server::transmission()
impl<T: Read + Seek> Write for Chd<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn handle_client<T>(file: &mut T, size: u64, mut stream: TcpStream) -> Result<()>
where
    T: Read + Seek + Write,
{
    let e = Export {
        size,
        readonly: false,
        ..Default::default()
    };
    handshake(&mut stream, &e)?;
    transmission(&mut stream, file)?;
    Ok(())
}

fn main() -> io::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .expect("Usage: rchd-nbd <chd-file>");
    let file = File::open(path)?;
    let mut chd = Chd::new(file);

    chd.read_header()?;
    let size = chd.len();

    let listener = TcpListener::bind("127.0.0.1:10809").unwrap();
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => match handle_client(&mut chd, size, stream) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("error: {}", e);
                }
            },
            Err(e) => {
                eprintln!("error: {}", e);
            }
        }
    }
    Ok(())
}
