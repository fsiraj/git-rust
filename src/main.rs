#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;

use flate2::bufread::ZlibDecoder;

fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.

    let args: Vec<String> = env::args().collect();
    eprintln!("{args:?}");

    if args[1] == "init" {
        fs::create_dir(".git").unwrap();
        fs::create_dir(".git/objects").unwrap();
        fs::create_dir(".git/refs").unwrap();
        fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
        println!("Initialized git directory")
    } else if args[1] == "cat-file" && args[2] == "-p" {
        let hash = &args[3];
        let (dir, file) = (&hash[..2], &hash[2..]);
        let path = format!(".git/objects/{dir}/{file}");
        eprintln!("Path is {path}");
        let file = File::open(path).expect("File could not be opened");
        let reader = BufReader::new(file);
        let mut decoder = ZlibDecoder::new(reader);
        let mut buffer = Vec::<u8>::new();
        let num_bytes = decoder
            .read_to_end(&mut buffer)
            .expect("File could not be read");
        eprintln!("Buffer contents ({num_bytes}): {buffer:?}");
        let space_idx = buffer
            .iter()
            .position(|el| *el == b' ')
            .expect("Did not find kind delimiter");
        let null_idx = buffer
            .iter()
            .position(|el| *el == b'\0')
            .expect("Did not find null characters");
        eprintln!("Space at {space_idx}, Null at {null_idx}");
        let kind = String::from_utf8_lossy(&buffer[..space_idx]).to_string();
        let content_len = String::from_utf8_lossy(&buffer[space_idx + 1..null_idx]).to_string();
        let content = String::from_utf8_lossy(&buffer[null_idx + 1..]);
        eprintln!("Decompressed into {kind} with {content_len} bytes of content: {content}");
        print!("{content}")
    } else {
        println!("unknown command: {}", args[1])
    }
}
