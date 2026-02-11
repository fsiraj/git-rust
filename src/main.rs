use core::fmt;
#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::io;
use std::io::Read;
use std::ops;
use std::path::Path;

use flate2::Compression;
use flate2::bufread::ZlibDecoder;
use flate2::bufread::ZlibEncoder;
use sha1::Digest;
use sha1::Sha1;

#[derive(Debug)]
struct Sha1Hash(String);

impl ops::Deref for Sha1Hash {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Display for Sha1Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug)]
enum GitObjectHeader {
    Blob,
    Tree,
}

impl GitObjectHeader {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Blob => "blob",
            Self::Tree => "tree",
        }
    }
}

#[derive(Debug)]
struct GitObject {
    header: GitObjectHeader,
    size: usize,
    content: Vec<u8>,
}

impl GitObject {
    pub fn hash(&self) -> Sha1Hash {
        let file_content = self.get_file_content();
        let mut hasher = Sha1::new();
        hasher.update(&file_content);
        Sha1Hash(format!("{:x}", hasher.finalize()))
    }

    pub fn write(&self) -> String {
        let file_content = self.get_file_content();
        let mut encoder = ZlibEncoder::new(&file_content[..], Compression::fast());
        let mut compressed = Vec::<u8>::new();
        encoder
            .read_to_end(&mut compressed)
            .expect("Compression failed");
        let hash = self.hash();
        let (dir, file) = (&hash[..2], &hash[2..]);
        let out_path_str = format!(".git/objects/{dir}/{file}");
        let out_path = Path::new(&out_path_str);
        fs::create_dir_all(
            out_path
                .parent()
                .expect("Git object must have parent folder in objects"),
        )
        .expect("Could not create a directory");
        fs::write(out_path, compressed).expect("Could not create or write to file");
        out_path_str
    }

    pub fn get_content_string(&self) -> String {
        String::from_utf8_lossy(&self.content).to_string()
    }

    fn get_file_content(&self) -> Vec<u8> {
        let mut result = Vec::<u8>::new();
        result.extend_from_slice(self.header.as_str().as_bytes());
        result.push(b' ');
        result.extend_from_slice(self.size.to_string().as_bytes());
        result.push(b'\0');
        result.extend_from_slice(&self.content);
        result
    }
}

impl From<Sha1Hash> for GitObject {
    fn from(hash: Sha1Hash) -> Self {
        // Read in file contents
        let (dir, file) = (&hash[..2], &hash[2..]);
        let path_str = format!(".git/objects/{dir}/{file}");
        let path = Path::new(&path_str);

        let file = fs::File::open(path).expect("File could not be opened");
        let reader = io::BufReader::new(file);
        let mut decoder = ZlibDecoder::new(reader);
        let mut buffer = Vec::<u8>::new();
        let _num_bytes = decoder
            .read_to_end(&mut buffer)
            .expect("File could not be read");

        // Parse file contents
        let space_idx = buffer
            .iter()
            .position(|el| *el == b' ')
            .expect("Did not find kind delimiter");
        let null_idx = buffer
            .iter()
            .position(|el| *el == b'\0')
            .expect("Did not find null characters");

        let header_str = str::from_utf8(&buffer[..space_idx]).expect("invalid utf-8 in header");
        let header = match header_str {
            "blob" => GitObjectHeader::Blob,
            "tree" => GitObjectHeader::Tree,
            _ => panic!("invalid git object header"),
        };
        let size_str =
            str::from_utf8(&buffer[space_idx + 1..null_idx]).expect("invalid utf-8 in size");
        let size = size_str.parse::<usize>().expect("invalid size");
        let content = buffer[null_idx + 1..].to_vec();

        Self {
            header,
            size,
            content,
        }
    }
}

impl From<&Path> for GitObject {
    fn from(path: &Path) -> Self {
        let content = fs::read(path).expect("File could not be opened or read");
        let size = content.len();
        let header = GitObjectHeader::Blob;
        Self {
            header,
            size,
            content,
        }
    }
}

fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.

    let args: Vec<String> = env::args().collect();
    eprintln!("{args:?}");

    if args[1] == "init" {
        //
        fs::create_dir(".git").unwrap();
        fs::create_dir(".git/objects").unwrap();
        fs::create_dir(".git/refs").unwrap();
        fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
        println!("Initialized git directory")
        //
    } else if args[1] == "hash-object" && args[2] == "-w" {
        //
        let in_path_str = &args[3];
        let in_path = Path::new(in_path_str);
        let git_object = GitObject::from(in_path);
        git_object.write();
        println!("{}", git_object.hash());
        //
    } else if args[1] == "cat-file" && args[2] == "-p" {
        //
        let hash = Sha1Hash(args[3].clone());
        let git_object = GitObject::from(hash);
        print!("{}", git_object.get_content_string());
        //
    } else if args[1] == "ls-tree" {
    } else {
        println!("unknown command: {}", args[1])
    }
}
