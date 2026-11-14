//! The identity of the code a build compiles, which build.rs sets as `PARTIMENTO_BUILD` and
//! each key of the artifact cache holds. build.rs includes this file as a module, and the
//! crate's tests include it too, so the tests check the identity that build.rs computes.

// The tests that read the package's files don't run on wasm, where the test runner doesn't open
// a directory for them, so there the functions that read them aren't called.
#![cfg_attr(target_family = "wasm", allow(dead_code))]

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

use super::sha256::Sha256;

/// The files of the package under `root` whose bytes its build's identity holds: each file under
/// src/, by its path from `root` with `/` between its parts, in the order of their paths, and
/// then Cargo.toml and, where the package has one, the Cargo.lock beside it. A read of Cargo.lock
/// that fails for any reason but a missing file is an error, so a build doesn't leave the lock
/// out of its identity.
pub(crate) fn package_files(root: &Path) -> io::Result<Vec<(String, Vec<u8>)>> {
    let mut files = Vec::new();
    collect(&root.join("src"), "src", &mut files)?;
    files.sort();
    files.push(("Cargo.toml".to_string(), fs::read(root.join("Cargo.toml"))?));
    match fs::read(root.join("Cargo.lock")) {
        Ok(bytes) => files.push(("Cargo.lock".to_string(), bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    Ok(files)
}

/// The identity of `files`: the SHA-256, in hex, of each file's path and bytes, in the order of
/// `files`, each after its length in 8 bytes, so that bytes moved from a path to its file's
/// bytes, or from one file to the next, make another identity.
pub(crate) fn identity(files: &[(String, Vec<u8>)]) -> String {
    let mut hasher = Sha256::default();
    for (path, bytes) in files {
        for part in [path.as_bytes(), bytes.as_slice()] {
            hasher.update(&(part.len() as u64).to_le_bytes());
            hasher.update(part);
        }
    }
    let mut hex = String::new();
    for byte in hasher.finish() {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
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
