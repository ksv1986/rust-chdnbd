extern crate cc;

fn main() {
    cc::Build::new()
        .file("lzma-19.00/src/Alloc.c")
        .file("lzma-19.00/src/LzFind.c")
        .file("lzma-19.00/src/LzmaEnc.c")
        .file("lzma-19.00/src/LzmaDec.c")
        .file("src/lzma.c")
        .include("lzma-19.00/include")
        .define("_7ZIP_ST", "1")
        .compile("liblzma.a");
}
