//! Not for merge: a check run of the s2n-quic-dc specification as it stands at
//! camshaft/s2n-quic commit 268baef5fa4514a7f4fb413fc292232d16d148df.
//!
//! `tests/s2n-quic-dc-spec/` holds the specification's source files at that commit, and
//! `tests/s2n-quic-dc-spec-blobs.txt` the git blob id of each of its 124 committed files.
//! The test checks that each copied source is the committed file, renders the sources,
//! compares each generated file with the committed one by its blob id, and runs the check
//! over the sources with the generated files.
#![cfg(not(target_family = "wasm"))]

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use partimento::{ByteVec, Options, Tree};

const BLOBS: &str = include_str!("s2n-quic-dc-spec-blobs.txt");
const PREFIX: &str = "dc/s2n-quic-dc/spec/";

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut message = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 80];
        for (slot, word) in w.iter_mut().zip(block.chunks(4)) {
            *slot = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = [0u8; 20];
    for (bytes, value) in out.chunks_mut(4).zip(h) {
        bytes.copy_from_slice(&value.to_be_bytes());
    }
    out
}

/// The git blob id of a file's bytes.
fn blob_id(data: &[u8]) -> String {
    let mut object = format!("blob {}\0", data.len()).into_bytes();
    object.extend_from_slice(data);
    let mut id = String::new();
    for byte in sha1(&object) {
        write!(id, "{byte:02x}").expect("the id is written");
    }
    id
}

#[test]
fn the_blob_id_of_a_known_text() {
    assert_eq!(blob_id(b""), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
    assert_eq!(blob_id(b"hello\n"), "ce013625030ba8dba906f756967f9e9ca394464a");
}

fn files_under(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    let mut pending: Vec<PathBuf> = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("the directory is read") {
            let path = entry.expect("the entry is read").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path.strip_prefix(dir).expect("the file is in dir");
            let name = relative.to_str().expect("the path is UTF-8").replace('\\', "/");
            found.push((name, std::fs::read(&path).expect("the file is read")));
        }
    }
    found.sort();
    found
}

#[test]
fn the_s2n_quic_dc_spec_at_268baef5() {
    let committed: BTreeMap<String, String> = BLOBS
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (path, id) = line.split_once(' ').expect("a path and an id");
            let path = path.strip_prefix(PREFIX).expect("a path of the spec");
            (path.to_string(), id.trim().to_string())
        })
        .collect();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/s2n-quic-dc-spec");
    let mut out = String::new();
    let mut problems = 0usize;

    let mut sources = Tree::default();
    for (path, bytes) in files_under(&root) {
        let id = blob_id(&bytes);
        match committed.get(&path) {
            Some(expected) if *expected == id => {}
            Some(expected) => {
                problems += 1;
                writeln!(out, "COPY DIFFERS: {path}: {id}, committed {expected}").unwrap();
            }
            None => {
                problems += 1;
                writeln!(out, "COPY NOT COMMITTED: {path}").unwrap();
            }
        }
        sources.files.insert(path, ByteVec::from(bytes));
    }
    writeln!(out, "sources read: {}", sources.files.len()).unwrap();

    let generated = match partimento::render(&sources) {
        Ok(generated) => generated,
        Err(findings) => {
            for finding in &findings {
                writeln!(out, "RENDER: {finding}").unwrap();
            }
            panic!("render refused the sources:\n{out}");
        }
    };
    writeln!(out, "files generated: {}", generated.files.len()).unwrap();

    for (path, bytes) in &generated.files {
        let id = blob_id(&bytes.copy_to_bytes().to_vec());
        match committed.get(path) {
            Some(expected) if *expected == id => {}
            Some(expected) => {
                problems += 1;
                writeln!(out, "DIFFERS: {path}: render writes {id}, committed {expected}").unwrap();
            }
            None => {
                problems += 1;
                writeln!(out, "NOT COMMITTED: {path}: render writes {id}").unwrap();
            }
        }
    }
    for path in committed.keys() {
        if !sources.files.contains_key(path) && !generated.files.contains_key(path) {
            problems += 1;
            writeln!(out, "COMMITTED, NOT GENERATED: {path}").unwrap();
        }
    }

    let mut full = sources.clone();
    for (path, bytes) in &generated.files {
        full.files.insert(path.clone(), bytes.clone());
    }
    let report = partimento::check_default(&full, &Options::default());
    for finding in &report.findings {
        writeln!(out, "CHECK: {finding}").unwrap();
    }
    writeln!(
        out,
        "check findings: {}, failed: {}",
        report.findings.len(),
        report.failed()
    )
    .unwrap();
    problems += report.findings.len();

    println!("{out}");
    assert_eq!(problems, 0, "\n{out}");
}
