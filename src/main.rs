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

#[derive(Debug, Clone)]
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
enum GitObjectKind {
    Blob,
    Tree,
}

impl GitObjectKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Blob => "blob",
            Self::Tree => "tree",
        }
    }
}

impl fmt::Display for GitObjectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug)]
struct GitObject {
    kind: GitObjectKind,
    size: usize,
    content: Vec<u8>,
}

impl GitObject {
    /// Computes the hash based on the file content
    pub fn hash(&self) -> Sha1Hash {
        let file_content = self.get_file_content();
        let mut hasher = Sha1::new();
        hasher.update(&file_content);
        Sha1Hash(format!("{:x}", hasher.finalize()))
    }

    /// Writes the GitObject to disk after computing it's file content and hash
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

    /// Returns only the content portion of the GitObject
    pub fn get_content_string(&self) -> String {
        String::from_utf8_lossy(&self.content).to_string()
    }

    /// Constructs the file content of the GitObject
    fn get_file_content(&self) -> Vec<u8> {
        let mut result = Vec::<u8>::new();
        result.extend_from_slice(self.kind.as_str().as_bytes());
        result.push(b' ');
        result.extend_from_slice(self.size.to_string().as_bytes());
        result.push(b'\0');
        result.extend_from_slice(&self.content);
        result
    }

    /// Parses the content as a Git Tree
    fn parse_as_tree(&self) -> Vec<TreeEntry> {
        let mut result = Vec::new();
        let mut start_idx = 0;
        while start_idx < self.size {
            // Find the delimiters
            let space_idx = start_idx
                + self
                    .content
                    .iter()
                    .skip(start_idx)
                    .position(|e| *e == b' ')
                    .expect("Did not find space character");
            let null_idx = start_idx
                + self
                    .content
                    .iter()
                    .skip(start_idx)
                    .position(|e| *e == b'\0')
                    .expect("Did not find null charcter");
            let end_idx = null_idx + 1 + 20;
            // Parse the entry
            let mode = &self.content[start_idx..space_idx];
            let mode = str::from_utf8(mode).expect("invalid bytes in mode");
            let mode = mode.parse::<u32>().expect("unable to parse as integer");

            let name = &self.content[space_idx + 1..null_idx];
            let name = str::from_utf8(name)
                .expect("invalid bytes in mode")
                .to_string();

            let hash = &self.content[null_idx + 1..end_idx];
            let hash = hash
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>();
            let hash = Sha1Hash(hash);

            let kind = GitObject::from(hash.clone()).kind;

            result.push(TreeEntry {
                mode,
                kind,
                hash,
                name,
            });
            start_idx = end_idx;
        }

        result
    }
}

impl From<Sha1Hash> for GitObject {
    /// Parses an existing GitObject from it's Sha1Hash
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

        let kind_str = str::from_utf8(&buffer[..space_idx]).expect("invalid utf-8 in kind");
        let kind = match kind_str {
            "blob" => GitObjectKind::Blob,
            "tree" => GitObjectKind::Tree,
            _ => panic!("invalid git object kind"),
        };
        let size_str =
            str::from_utf8(&buffer[space_idx + 1..null_idx]).expect("invalid utf-8 in size");
        let size = size_str.parse::<usize>().expect("invalid size");
        let content = buffer[null_idx + 1..].to_vec();

        Self {
            kind,
            size,
            content,
        }
    }
}

impl From<&Path> for GitObject {
    /// Constructs a Git Blob from a Path to any file
    fn from(path: &Path) -> Self {
        let content = fs::read(path).expect("File could not be opened or read");
        let size = content.len();
        let kind = GitObjectKind::Blob;
        Self {
            kind,
            size,
            content,
        }
    }
}

struct TreeEntry {
    mode: u32,
    kind: GitObjectKind,
    hash: Sha1Hash,
    name: String,
}

impl fmt::Display for TreeEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:06} {} {}    {}",
            self.mode, self.kind, self.hash, self.name
        )
    }
}

fn main() {
    // You can use print statements as follows for debugging, they'll be visible when running tests.

    let args: Vec<String> = env::args().collect();

    if args[1] == "init" {
        //
        fs::create_dir(".git").unwrap();
        fs::create_dir(".git/objects").unwrap();
        fs::create_dir(".git/refs").unwrap();
        fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
        println!("Initialized git directory");
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
        //
        let (name_only, hash) = if args[2] == "--name-only" {
            (true, &args[3])
        } else {
            (false, &args[2])
        };
        let hash = Sha1Hash(hash.clone());
        let git_object = GitObject::from(hash);
        let tree = git_object.parse_as_tree();
        for entry in tree {
            if name_only {
                println!("{}", entry.name);
            } else {
                println!("{entry}");
            }
        }
        //
    } else {
        println!("unknown command: {}", args[1]);
    }
}
