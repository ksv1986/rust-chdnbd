extern crate chd;
extern crate nbd;

use std::fs::File;
use std::io::{BufReader, Read, Result, Seek, SeekFrom, Write};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener, TcpStream};

use chd::read::ChdReader;
use chd::Chd;
use nbd::server::{handshake, transmission, Export};

fn handle_client<T>(file: &mut T, size: u64, mut stream: TcpStream) -> Result<()>
where
    T: Read + Write + Seek,
{
    let address = stream
        .local_addr()
        .unwrap_or(SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(0, 0, 0, 0),
            0,
        )));
    println!("accepted new client from {}", address);
    handshake(&mut stream, |_name| {
        Ok(Export {
            size,
            readonly: true,
            ..Default::default()
        })
    })?;
    transmission(&mut stream, file)?;
    Ok(())
}

struct ChdWriter<F: Read + Seek>(ChdReader<F>);

impl<F: Read + Seek> Read for ChdWriter<F> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.0.read(buf)
    }
}

impl<F: Read + Seek> Seek for ChdWriter<F> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.0.seek(pos)
    }
}

impl<F: Read + Seek> Write for ChdWriter<F> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

fn main() -> Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .expect("Usage: chdnbd <chd-file>");

    let f = BufReader::new(File::open(path)?);
    let chd = Chd::open(f, None)?;
    let size = chd.header().logical_bytes();

    let reader = ChdReader::new(chd);
    let mut writer = ChdWriter(reader);

    let listener = TcpListener::bind("127.0.0.1:10809").unwrap();
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => match handle_client(&mut writer, size, stream) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("error: {}", e);
                }
            },
            Err(_) => {
                println!("Error");
            }
        }
    }
    Ok(())
}
