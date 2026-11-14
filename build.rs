//! Sets `PARTIMENTO_BUILD`, the identity of the code a build compiles, which each key of the
//! artifact cache holds (src/artifact.rs): the SHA-256 of each file under src/, by its path,
//! and of the Cargo.lock beside Cargo.toml where the package holds one, each after its length
//! in 8 bytes, so that a build whose code differs, or whose lock holds another version of a
//! dependency such as the `toml` crate, keeps its artifacts under other keys.

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

#[allow(dead_code)]
#[path = "src/artifact/sha256.rs"]
mod sha256;

fn main() {
    // Cargo runs a build script in its package's root, which the paths below start from.
    let mut files = Vec::new();
    if let Err(error) = collect(Path::new("src"), "src", &mut files) {
        panic!("build.rs reads each file under src: {error}");
    }
    files.sort();
    if let Ok(bytes) = fs::read("Cargo.lock") {
        println!("cargo:rerun-if-changed=Cargo.lock");
        files.push(("Cargo.lock".to_string(), bytes));
    }
    println!("cargo:rerun-if-changed=src");
    let mut hasher = sha256::Sha256::default();
    for (path, bytes) in &files {
        for part in [path.as_bytes(), bytes.as_slice()] {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
    }
    let mut identity = String::new();
    for byte in hasher.finish() {
        let _ = write!(identity, "{byte:02x}");
    }
    println!("cargo:rustc-env=PARTIMENTO_BUILD={identity}");
}

/// Adds each file under `dir`, whose path from the package's root is `name`, to `files`: its path
/// from the package's root, with `/` between its parts, and its bytes.
fn collect(dir: &Path, name: &str, files: &mut Vec<(String, Vec<u8>)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = format!("{name}/{}", entry.file_name().to_string_lossy());
        let path = entry.path();
        if path.is_dir() {
            collect(&path, &name, files)?;
        } else {
            files.push((name, fs::read(path)?));
        }
    }
    Ok(())
}
