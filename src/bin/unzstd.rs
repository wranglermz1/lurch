fn main() {
    let a: Vec<String> = std::env::args().collect();
    let src = std::fs::read(&a[1]).unwrap();
    let t = std::time::Instant::now();
    let out = lurch::assets::zstd::decompress(&src).unwrap();
    eprintln!("{} -> {} bytes in {:?}", src.len(), out.len(), t.elapsed());
    std::fs::write(&a[2], out).unwrap();
}
