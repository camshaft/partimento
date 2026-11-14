//! Sets `PARTIMENTO_BUILD`, the identity of the code a build compiles, which each key of the
//! artifact cache holds (src/artifact.rs). src/artifact/build_identity.rs computes it from each
//! file under src/ and the Cargo.lock beside Cargo.toml, and the crate's tests include that file
//! too, so they check the identity this script sets.

use std::path::Path;

#[allow(dead_code)]
#[path = "src/artifact/sha256.rs"]
mod sha256;

#[path = "src/artifact/build_identity.rs"]
mod build_identity;

fn main() {
    // Cargo runs a build script in its package's root.
    let files = build_identity::package_files(Path::new("."))
        .unwrap_or_else(|error| panic!("build.rs reads the package's files: {error}"));
    if files.last().is_some_and(|(path, _)| path == "Cargo.lock") {
        println!("cargo:rerun-if-changed=Cargo.lock");
    }
    println!("cargo:rerun-if-changed=src");
    let identity = build_identity::identity(&files);
    println!("cargo:rustc-env=PARTIMENTO_BUILD={identity}");
}
