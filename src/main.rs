extern crate nbd;

use std::io;
use std::io::{Cursor, Read, Result, Seek, Write};
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

fn handle_client(data: &mut [u8], mut stream: TcpStream) -> Result<()> {
    let e = Export {
        size: data.len() as u64,
        readonly: false,
        ..Default::default()
    };
    let pseudofile = Cursor::new(data);
    let chd = chd::Chd::new(pseudofile);
    handshake(&mut stream, &e)?;
    transmission(&mut stream, chd)?;
    Ok(())
}

fn main() {
    let mut data = vec![0; 1_474_560];
    let listener = TcpListener::bind("127.0.0.1:10809").unwrap();

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => match handle_client(&mut data, stream) {
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
}
