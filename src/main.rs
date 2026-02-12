use core::fmt;
#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::io;
use std::io::Read;
use std::ops;
use std::path::Path;

use clap::{Parser, Subcommand};
use chrono::Local;
use chrono::Offset;
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

impl Sha1Hash {
    fn as_bytes(&self) -> [u8; 20] {
        let mut bytes = [0u8; 20];
        for (i, byte) in bytes.iter_mut().enumerate() {
            let idx = i * 2;
            *byte =
                u8::from_str_radix(&self[idx..idx + 2], 16).expect("invalid hex in sha-1 string");
        }
        bytes
    }
}

#[derive(Debug, Clone)]
enum GitObjectKind {
    Blob,
    Tree,
    Commit,
}

impl GitObjectKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Blob => "blob",
            Self::Tree => "tree",
            Self::Commit => "commit",
        }
    }
}

impl fmt::Display for GitObjectKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
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

impl TreeEntry {
    /// Returns the bytes of the TreeEntry for the Tree GitObject file
    fn serialize(&self) -> Vec<u8> {
        let mut result = Vec::<u8>::new();
        let mode_str = self.mode.to_string();
        result.extend_from_slice(mode_str.as_bytes());
        result.push(b' ');
        result.extend_from_slice(self.name.as_bytes());
        result.push(b'\0');
        result.extend_from_slice(&self.hash.as_bytes());
        result
    }
}

#[derive(Debug)]
struct GitObject {
    kind: GitObjectKind,
    size: usize,
    content: Vec<u8>,
}

impl GitObject {
    /// Constructs the file content of the GitObject
    fn serialize(&self) -> Vec<u8> {
        let mut result = Vec::<u8>::new();
        result.extend_from_slice(self.kind.as_str().as_bytes());
        result.push(b' ');
        result.extend_from_slice(self.size.to_string().as_bytes());
        result.push(b'\0');
        result.extend_from_slice(&self.content);
        result
    }

    /// Computes the hash based on the file content
    pub fn hash(&self) -> Sha1Hash {
        let file_content = self.serialize();
        let mut hasher = Sha1::new();
        hasher.update(&file_content);
        Sha1Hash(format!("{:x}", hasher.finalize()))
    }

    /// Writes the GitObject to disk after computing it's file content and hash
    pub fn write(&self) -> String {
        let file_content = self.serialize();
        let mut encoder = ZlibEncoder::new(&file_content[..], Compression::fast());
        let mut compressed = Vec::<u8>::new();
        encoder
            .read_to_end(&mut compressed)
            .expect("compression failed");
        let hash = self.hash();
        let (dir, file) = (&hash[..2], &hash[2..]);
        let out_path_str = format!(".git/objects/{dir}/{file}");
        let out_path = Path::new(&out_path_str);
        fs::create_dir_all(
            out_path
                .parent()
                .expect("git object must have parent folder in objects"),
        )
        .expect("could not create a directory");
        fs::write(out_path, compressed).expect("could not create or write to file");
        out_path_str
    }

    /// Returns only the content portion of the GitObject
    fn parse_as_blob(&self) -> String {
        // assert!(matches!(self.kind, GitObjectKind::Blob));
        String::from_utf8_lossy(&self.content).to_string()
    }

    /// Parses the content as a Git Tree
    fn parse_as_tree(&self) -> Vec<TreeEntry> {
        assert!(matches!(self.kind, GitObjectKind::Tree));
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
                    .expect("did not find space character");
            let null_idx = start_idx
                + self
                    .content
                    .iter()
                    .skip(start_idx)
                    .position(|e| *e == b'\0')
                    .expect("did not find null charcter");
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

        let file = fs::File::open(path).expect("file could not be opened");
        let reader = io::BufReader::new(file);
        let mut decoder = ZlibDecoder::new(reader);
        let mut buffer = Vec::<u8>::new();
        let _num_bytes = decoder
            .read_to_end(&mut buffer)
            .expect("file could not be read");

        // Parse file contents
        let space_idx = buffer
            .iter()
            .position(|el| *el == b' ')
            .expect("did not find kind delimiter");
        let null_idx = buffer
            .iter()
            .position(|el| *el == b'\0')
            .expect("did not find null characters");

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
    /// Constructs a Git Blob or Tree from a Path to any file or directory
    fn from(path: &Path) -> Self {
        if path.is_file() {
            // Blob
            let kind = GitObjectKind::Blob;
            let content = fs::read(path).expect("file could not be opened or read");
            let size = content.len();
            Self {
                kind,
                size,
                content,
            }
        } else {
            // Tree
            let kind = GitObjectKind::Tree;
            // Construct tree entries
            let mut tree_entries = Vec::<TreeEntry>::new();
            for entry in fs::read_dir(path).expect("unable to read directory") {
                let entry = entry.expect("unable to read entry in directory");
                let entry_path = entry.path();
                let name = entry_path
                    .file_name()
                    .expect("expected a filename")
                    .to_string_lossy()
                    .to_string();
                if name == ".git" {
                    continue;
                }
                if entry_path.is_dir() && fs::read_dir(&entry_path).unwrap().next().is_none() {
                    continue;
                }
                let mode = if entry_path.is_dir() { 40000 } else { 100644 };
                let git_object = GitObject::from(entry_path.as_path());
                git_object.write();
                let hash = git_object.hash();
                let tree_entry = TreeEntry {
                    mode,
                    kind: git_object.kind.clone(),
                    hash,
                    name,
                };
                tree_entries.push(tree_entry);
            }
            // Sort them and then generate content bytes
            tree_entries.sort_by_key(|entry| entry.name.clone());
            let mut content = Vec::<u8>::new();
            for entry in tree_entries {
                content.extend_from_slice(&entry.serialize());
            }
            let size = content.len();
            Self {
                kind,
                size,
                content,
            }
        }
    }
}

impl From<(Sha1Hash, Option<Sha1Hash>, String)> for GitObject {
    fn from(hashes: (Sha1Hash, Option<Sha1Hash>, String)) -> Self {
        let (tree_hash, parent_hash, message) = hashes;
        let kind = GitObjectKind::Commit;
        let mut content = Vec::<u8>::new();
        content.extend_from_slice(format!("tree {}\n", tree_hash).as_bytes());
        if let Some(parent_hash) = parent_hash {
            content.extend_from_slice(format!("parent {}\n", parent_hash).as_bytes());
        }
        let timestamp = get_timestamp_str();
        for field in ["author", "committer"] {
            content.extend_from_slice(
                format!("{} fsiraj <fsiraj@git.com> {}\n", field, timestamp).as_bytes(),
            );
        }
        content.push(b'\n');
        content.extend_from_slice(message.as_bytes());
        content.push(b'\n');
        let size = content.len();
        Self {
            kind,
            size,
            content,
        }
    }
}

fn get_timestamp_str() -> String {
    let now = Local::now();
    let timestamp = now.timestamp();
    let offset = now.offset().fix().local_minus_utc();
    let hours = offset / 3600;
    let minutes = (offset.abs() % 3600) / 60;
    let timezone = format!("{:+03}{:02}", hours, minutes);
    format!("{} {}", timestamp, timezone)
}

#[derive(Parser)]
#[command(name = "git")]
#[command(about = "A simple git implementation")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new git repository
    Init,
    /// Compute object ID and optionally creates a blob from a file
    HashObject {
        /// Write the object to the database
        #[arg(short = 'w')]
        write: bool,
        /// The file to hash
        file: String,
    },
    /// Provide content for repository objects
    CatFile {
        /// Pretty-print the contents
        #[arg(short = 'p')]
        pretty_print: bool,
        /// Object hash
        hash: String,
    },
    /// List the contents of a tree object
    LsTree {
        /// List only filenames
        #[arg(long)]
        name_only: bool,
        /// Tree hash
        hash: String,
    },
    /// Create a tree object from the current directory
    WriteTree,
    /// Create a new commit object
    CommitTree {
        /// Tree hash
        tree_hash: String,
        /// Parent commit hash
        #[arg(short = 'p')]
        parent: Option<String>,
        /// Commit message
        #[arg(short = 'm')]
        message: String,
    },
}


fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            fs::create_dir(".git").unwrap();
            fs::create_dir(".git/objects").unwrap();
            fs::create_dir(".git/refs").unwrap();
            fs::write(".git/HEAD", "ref: refs/heads/main\n").unwrap();
            println!("Initialized git directory");
        }
        Commands::HashObject { write, file } => {
            let in_path = Path::new(&file);
            let git_object = GitObject::from(in_path);
            println!("{}", git_object.hash());
            if write {
                git_object.write();
            }
        }
        Commands::CatFile { pretty_print, hash } => {
            if pretty_print {
                let hash = Sha1Hash(hash);
                let git_object = GitObject::from(hash);
                print!("{}", git_object.parse_as_blob());
            }
        }
        Commands::LsTree { name_only, hash } => {
            let hash = Sha1Hash(hash);
            let git_object = GitObject::from(hash);
            let tree = git_object.parse_as_tree();
            for entry in tree {
                if name_only {
                    println!("{}", entry.name);
                } else {
                    println!("{entry}");
                }
            }
        }
        Commands::WriteTree => {
            let root = Path::new(".");
            let tree = GitObject::from(root);
            tree.write();
            println!("{}", tree.hash());
        }
        Commands::CommitTree {
            tree_hash,
            parent,
            message,
        } => {
            let tree_hash = Sha1Hash(tree_hash);
            let parent_hash = parent.map(Sha1Hash);
            let commit = GitObject::from((tree_hash, parent_hash, message));
            commit.write();
            println!("{}", commit.hash());
        }
    }
}
