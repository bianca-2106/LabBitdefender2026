use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use zip::ZipArchive;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let zip_path = Path::new(&args[1]);
    let file = File::open(&zip_path)?;
    let reader = BufReader::new(file);

    let mut archive = ZipArchive::new(reader)?;

    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        println!("File {}: {}", i, file.name());
    }
    Ok(())
}