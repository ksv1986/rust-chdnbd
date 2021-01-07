extern crate nbd;

use std::fs::File;
use std::io;
use std::io::{Read, Result, Seek, SeekFrom, Write};
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

fn print_summary<T: Read + Seek>(chd: &Chd<T>, file_size: u64) {
    println!("File size: {}", file_size);
    println!("CHD version: {}", chd.version());
    println!("CHD size: {}", chd.len());
    print!("Compression:");
    for i in 0..4 {
        match chd.codec_name(i) {
            Some(s) => print!(" {}", s),
            None => break,
        }
    }
    println!("");
    println!(
        "Ratio: {:.1}%",
        1e2 * (file_size as f32) / (chd.len() as f32)
    );
    let distr = chd.compression_distribution();
    println!("Hunk size: {}", chd.hunk_size());
    println!("Hunk count: {}", chd.hunk_count());
    println!("Hunk compression distribution:");
    for (i, n) in distr.iter().enumerate() {
        if *n > 0 {
            println!("  {}: {}", chd.compression_name(i as u8), n);
        }
    }
}

fn main() -> io::Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .expect("Usage: rchd-nbd <chd-file>");
    let mut file = File::open(path)?;
    let file_size = file.seek(SeekFrom::End(0))?;

    let mut chd = Chd::new(file);
    chd.read_header()?;
    print_summary(&chd, file_size);
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
