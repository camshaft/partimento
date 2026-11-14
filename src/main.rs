//! The `partimento` command: reads specification trees from disk and runs the library on them.
//!
//! `partimento check [--root <dir>] [--format text|json] [--final] [--cache <dir>]
//! [--formulas <file>]` prints each finding and exits with 0 where no finding fails the check, 1
//! where one does, and 2 where the arguments are wrong or a file can't be read or written. With
//! `--cache`, it keeps what the check builds from each section file in `<dir>`, a file for each
//! key, and reads it back in later checks. With `--formulas`, it writes the formula set that
//! checks 1 and 2 read to `<file>` as JSON before it prints the findings: each atom, with its
//! RFC, its section file, its level and its polarity, and each formula and condition, with the
//! requirement, the file and the line it comes from.
//!
//! `partimento diff <base> <head> [--format text|json]` prints each requirement added, changed or
//! retired from the tree at `<base>` to the tree at `<head>` and exits with 0. Where either tree
//! fails a format rule, it prints that tree's failing findings instead and exits with 1, and it
//! exits with 2 where the arguments are wrong or a file can't be read.
//!
//! `partimento render [--root <dir>] [--format text|json]` writes each generated file that differs
//! from the tree, removes each file under the generated directories that the render doesn't
//! generate, prints nothing and exits with 0. Where the set breaks a format rule, it prints the
//! findings of the format rules, writes nothing and exits with 1; where the arguments are wrong or
//! a file can't be read or written, it exits with 2.

use std::collections::BTreeMap;
use std::fmt::{Display, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

use partimento::logic::FormulaSet;
use partimento::{config, json, ByteVec, Cache, Change, Finding, Key, Options, Report, Tree};

const USAGE: &str = concat!(
    "usage: partimento check [--root <dir>] [--format text|json] [--final] [--cache <dir>]\n",
    "                        [--formulas <file>]\n",
    "       partimento render [--root <dir>] [--format text|json]\n",
    "       partimento diff <base> <head> [--format text|json]",
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Json,
}

enum Command {
    /// `check`, of the tree at a root, with the directory of its cache where `--cache` names one
    /// and the file of its formula set where `--formulas` names one.
    Check(PathBuf, Options, Option<PathBuf>, Option<PathBuf>),
    /// `render`, of the tree at a root.
    Render(PathBuf),
    /// `diff`, from the tree at a base to the tree at a head.
    Diff(PathBuf, PathBuf),
}

struct Args {
    command: Command,
    format: Format,
}

fn main() -> ExitCode {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("partimento: {message}");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("partimento: {message}");
            ExitCode::from(2)
        }
    }
}

/// Runs the command and prints its output; the exit code, or why a file can't be read.
fn run(args: &Args) -> Result<ExitCode, String> {
    let (output, failed) = match &args.command {
        Command::Check(root, options, cache, formulas) => {
            let report = check(&read_tree(root)?, options, cache.as_deref())?;
            if let Some(file) = formulas {
                let json = formulas_json(&report.formulas);
                fs::write(file, json).map_err(|error| io_error(file, &error))?;
            }
            let output = findings_output(&report.findings, args.format);
            (output, report.failed())
        }
        Command::Diff(base, head) => {
            let (base, head) = (read_tree(base)?, read_tree(head)?);
            match partimento::diff(&base, &head) {
                Ok(changes) => (changes_output(&changes, args.format), false),
                Err(findings) => (findings_output(&findings, args.format), true),
            }
        }
        Command::Render(root) => return render(root, &read_tree(root)?, args.format),
    };
    print!("{output}");
    Ok(ExitCode::from(u8::from(failed)))
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    match args.next().as_deref() {
        Some("check") => parse_check(args),
        Some("render") => parse_render(args),
        Some("diff") => parse_diff(args),
        Some(command) => Err(format!("`{command}` isn't a command")),
        None => Err("the command is missing".to_string()),
    }
}

fn parse_check(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut root = PathBuf::from(".");
    let mut format = Format::Text;
    let mut options = Options::default();
    let mut cache = None;
    let mut formulas = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                let dir = args.next().ok_or("--root needs a directory")?;
                root = PathBuf::from(dir);
            }
            "--format" => format = parse_format(args.next())?,
            "--final" => options.final_pass = true,
            "--cache" => {
                let dir = args.next().ok_or("--cache needs a directory")?;
                cache = Some(PathBuf::from(dir));
            }
            "--formulas" => {
                // A value that starts with `--` is the next argument, not a file.
                let file = args.next().filter(|file| !file.starts_with("--"));
                let file = file.ok_or("--formulas needs a file")?;
                formulas = Some(PathBuf::from(file));
            }
            _ => return Err(format!("`{arg}` isn't an argument of `check`")),
        }
    }
    let command = Command::Check(root, options, cache, formulas);
    Ok(Args { command, format })
}

fn parse_render(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut root = PathBuf::from(".");
    let mut format = Format::Text;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                let dir = args.next().ok_or("--root needs a directory")?;
                root = PathBuf::from(dir);
            }
            "--format" => format = parse_format(args.next())?,
            _ => return Err(format!("`{arg}` isn't an argument of `render`")),
        }
    }
    let command = Command::Render(root);
    Ok(Args { command, format })
}

fn parse_diff(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut format = Format::Text;
    let mut dirs = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => format = parse_format(args.next())?,
            _ if arg.starts_with("--") => {
                return Err(format!("`{arg}` isn't an argument of `diff`"));
            }
            _ => dirs.push(PathBuf::from(&arg)),
        }
    }
    let Ok([base, head]) = <[PathBuf; 2]>::try_from(dirs) else {
        return Err("`diff` needs two directories, <base> and <head>".to_string());
    };
    let command = Command::Diff(base, head);
    Ok(Args { command, format })
}

fn parse_format(value: Option<String>) -> Result<Format, String> {
    match value.as_deref() {
        Some("text") => Ok(Format::Text),
        Some("json") => Ok(Format::Json),
        _ => Err("--format needs `text` or `json`".to_string()),
    }
}

/// Renders the tree, and writes its generated files under `root`, or prints the findings that
/// stop the render; the exit code, or why a file can't be written.
fn render(root: &Path, tree: &Tree, format: Format) -> Result<ExitCode, String> {
    let generated = match partimento::render(tree) {
        Ok(generated) => generated,
        Err(findings) => {
            print!("{}", findings_output(&findings, format));
            return Ok(ExitCode::from(1));
        }
    };
    // A tree that renders holds a configuration that loads.
    if let Some(config) = config::load(tree, &mut Vec::new()) {
        write(root, tree, &generated, &config.paths)?;
    }
    Ok(ExitCode::SUCCESS)
}

/// Removes each file of the tree under the generated directories of `paths` that the render
/// doesn't generate, the files the drift rule reports as stale, with each directory above it that
/// the removal leaves empty, and writes each generated file that the tree doesn't hold byte for
/// byte.
fn write(root: &Path, tree: &Tree, generated: &Tree, paths: &config::Paths) -> Result<(), String> {
    for path in tree.files.keys() {
        let stale = config::is_generated(&paths.rendered, &paths.requirements, path)
            && !generated.files.contains_key(path);
        if !stale {
            continue;
        }
        let file = root.join(path);
        fs::remove_file(&file).map_err(|error| io_error(&file, &error))?;
        remove_empty(root, path, paths)?;
    }
    for (path, bytes) in &generated.files {
        if tree.files.get(path) == Some(bytes) {
            continue;
        }
        let file = root.join(path);
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).map_err(|error| io_error(parent, &error))?;
        }
        let data = bytes.copy_to_bytes().to_vec();
        fs::write(&file, data).map_err(|error| io_error(&file, &error))?;
    }
    Ok(())
}

/// Removes each directory above `path` that is empty, from the nearest, up to the generated
/// directory of `paths` that holds it, which stays: that directory isn't under a generated
/// directory itself.
fn remove_empty(root: &Path, path: &str, paths: &config::Paths) -> Result<(), String> {
    let mut rest = path;
    while let Some((parent, _)) = rest.rsplit_once('/') {
        if !config::is_generated(&paths.rendered, &paths.requirements, parent) {
            break;
        }
        let dir = root.join(parent);
        let mut entries = fs::read_dir(&dir).map_err(|error| io_error(&dir, &error))?;
        if entries.next().is_some() {
            break;
        }
        fs::remove_dir(&dir).map_err(|error| io_error(&dir, &error))?;
        rest = parent;
    }
    Ok(())
}

/// The backend `check_default` checks with, which a check with a cache checks with too.
type Backend = partimento::solver::DefaultBackend;

/// Checks the tree, and keeps the artifacts the check builds in `cache` where it names a
/// directory.
fn check(tree: &Tree, options: &Options, cache: Option<&Path>) -> Result<Report, String> {
    let Some(dir) = cache else {
        return Ok(partimento::check_default(tree, options));
    };
    let cache = DiskCache::open(dir)?;
    Ok(partimento::check_cached::<Backend, _>(
        tree, options, &cache,
    ))
}

/// A cache on disk: a directory that holds each artifact in a file named by its key in hex.
struct DiskCache {
    dir: PathBuf,
}

impl DiskCache {
    /// The cache in `dir`, which it makes where it isn't there.
    fn open(dir: &Path) -> Result<DiskCache, String> {
        let failed = |error: io::Error| io_error(dir, &error);
        fs::create_dir_all(dir).map_err(failed)?;
        let dir = dir.to_path_buf();
        Ok(DiskCache { dir })
    }

    /// The file of the artifact kept under `key`.
    fn file(&self, key: &Key) -> PathBuf {
        let mut name = String::new();
        for byte in key.0 {
            let _ = write!(name, "{byte:02x}");
        }
        self.dir.join(name)
    }
}

impl Cache for DiskCache {
    fn get(&self, key: &Key) -> Option<ByteVec> {
        let bytes = fs::read(self.file(key)).ok()?;
        Some(ByteVec::from(bytes))
    }

    /// Writes the artifact to a file of its own and renames that file to the key's, so that a
    /// read of the key's file reads a whole artifact. Where the write fails, the cache stays as it
    /// was, and a later check builds the artifact again.
    fn put(&self, key: Key, artifact: ByteVec) {
        let file = self.file(&key);
        let temporary = file.with_extension(format!("{}.tmp", std::process::id()));
        let written = fs::write(&temporary, artifact.copy_to_bytes());
        let renamed = written.and_then(|()| fs::rename(&temporary, &file));
        if renamed.is_err() {
            let _ = fs::remove_file(&temporary);
        }
    }
}

/// Reads spec.toml and each file under the set's directories that spec.toml lists.
fn read_tree(root: &Path) -> Result<Tree, String> {
    let mut tree = Tree::default();
    let spec = root.join(config::FILE);
    if is_link(&spec) {
        return Err(link(&spec));
    }
    let bytes = fs::read(&spec).map_err(|error| io_error(&spec, &error))?;
    let file = config::FILE.to_string();
    tree.files.insert(file, ByteVec::from(bytes));

    // A configuration with findings leaves the tree at spec.toml, and the check reports them.
    let Some(config) = config::load(&tree, &mut Vec::new()) else {
        return Ok(tree);
    };
    let paths = &config.paths;
    for dir in [&paths.rfcs, &paths.rendered, &paths.requirements] {
        // A link at any part of the path, and not only at its last, would take the read outside
        // the tree, so each part is checked from the root.
        let mut part = root.to_path_buf();
        for name in dir.split('/') {
            part.push(name);
            if is_link(&part) {
                return Err(link(&part));
            }
        }
        read_dir(root, dir, &mut tree.files)?;
    }
    Ok(tree)
}

/// Reads each file under `dir`, a directory relative to `root`, where it is there. It reads only
/// regular files and directories: a symbolic link, or an entry of any other kind, stops the read,
/// so that the command reads nothing outside the tree.
fn read_dir(root: &Path, dir: &str, files: &mut BTreeMap<String, ByteVec>) -> Result<(), String> {
    let path = root.join(dir);
    if is_link(&path) {
        return Err(link(&path));
    }
    let entries = match fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(io_error(&path, &error)),
    };
    let failed = |error: io::Error| io_error(&path, &error);
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(failed)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            let message = "a file's name isn't UTF-8";
            return Err(format!("{}: {message}", path.display()));
        };
        let kind = entry.file_type().map_err(failed)?;
        if kind.is_symlink() {
            return Err(link(&entry.path()));
        }
        if !kind.is_dir() && !kind.is_file() {
            let message = "neither a file nor a directory";
            return Err(format!("{}: {message}", entry.path().display()));
        }
        names.push((name.to_string(), kind.is_dir()));
    }
    for (name, is_dir) in names {
        let relative = format!("{dir}/{name}");
        if is_dir {
            read_dir(root, &relative, files)?;
        } else {
            let file = root.join(&relative);
            let bytes = fs::read(&file).map_err(|error| io_error(&file, &error))?;
            files.insert(relative, ByteVec::from(bytes));
        }
    }
    Ok(())
}

fn io_error(path: &Path, error: &io::Error) -> String {
    format!("{}: {error}", path.display())
}

/// Whether `path` is a symbolic link, which the command doesn't follow.
fn is_link(path: &Path) -> bool {
    matches!(fs::symlink_metadata(path), Ok(metadata) if metadata.file_type().is_symlink())
}

fn link(path: &Path) -> String {
    format!(
        "{}: a symbolic link, which the command doesn't follow",
        path.display()
    )
}

/// The findings as text, one a line, or as a JSON array.
fn findings_output(findings: &[Finding], format: Format) -> String {
    match format {
        Format::Text => lines(findings),
        Format::Json => json_array(findings.iter().map(json_finding)),
    }
}

/// The changes as text, one a line, or as a JSON array.
fn changes_output(changes: &[Change], format: Format) -> String {
    match format {
        Format::Text => lines(changes),
        Format::Json => json_array(changes.iter().map(json_change)),
    }
}

fn lines(items: &[impl Display]) -> String {
    let mut output = String::new();
    for item in items {
        let _ = writeln!(output, "{item}");
    }
    output
}

/// The items as a JSON array, one item an element, each on a line of its own.
fn json_array(items: impl Iterator<Item = String>) -> String {
    let items: Vec<String> = items.collect();
    format!("{}\n", json_list(&items))
}

/// The items as a JSON array, one item an element, each on a line of its own, without a line
/// break after the array.
fn json_list(items: &[String]) -> String {
    if items.is_empty() {
        return "[]".to_string();
    }
    format!("[\n{}\n]", items.join(",\n"))
}

fn json_finding(finding: &Finding) -> String {
    let requirement = match &finding.requirement {
        Some(requirement) => json_string(requirement),
        None => "null".to_string(),
    };
    let severity = json_string(finding.severity.as_str());
    let rule = json_string(&finding.rule);
    let file = json_string(&finding.file);
    let line = finding.line;
    let message = json_string(&finding.message);
    format!(
        "{{\"severity\":{severity},\"rule\":{rule},\"file\":{file},\"line\":{line},\
         \"requirement\":{requirement},\"message\":{message}}}"
    )
}

fn json_change(change: &Change) -> String {
    let kind = json_string(change.kind.as_str());
    let requirement = json_string(&change.requirement);
    let binds_to = json_string(change.binds_to.as_str());
    format!("{{\"kind\":{kind},\"requirement\":{requirement},\"binds_to\":{binds_to}}}")
}

/// The formula set as a JSON object of three arrays, each element on a line of its own: its
/// atoms, its formulas and its conditions. The library's `json` module writes each element, in
/// the shape of serde's derived `Serialize`, and a formula with a stack on the heap.
fn formulas_json(set: &FormulaSet) -> String {
    let atoms: Vec<String> = set.atoms.iter().map(json::atom).collect();
    let formulas: Vec<String> = set.formulas.iter().map(json::entry).collect();
    let conditions: Vec<String> = set.conditions.iter().map(json::condition).collect();
    let (atoms, formulas) = (json_list(&atoms), json_list(&formulas));
    let conditions = json_list(&conditions);
    format!(
        "{{\n\"atoms\": {atoms},\n\"formulas\": {formulas},\n\"conditions\": {conditions}\n}}\n"
    )
}

fn json_string(text: &str) -> String {
    let mut output = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(output, "\\u{:04x}", u32::from(c));
            }
            c => output.push(c),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use partimento::logic::{Entry, Formula, Origin};

    /// A formula of 100,000 nested `!` writes as `not` objects around its atom, without
    /// overflowing the stack.
    #[test]
    fn formulas_json_writes_a_deep_formula() {
        const DEPTH: usize = 100_000;
        let mut formula = Formula::atom("EX-0001-001");
        for _ in 0..DEPTH {
            formula = formula.not();
        }
        let set = FormulaSet {
            atoms: Vec::new(),
            formulas: vec![Entry {
                formula,
                origin: Origin {
                    requirement: "EX-0001-001".to_owned(),
                    file: "rfcs/1-example/01-example.toml".to_owned(),
                    line: 1,
                },
                relation: None,
            }],
            conditions: Vec::new(),
        };
        let opens = "{\"not\":".repeat(DEPTH);
        let closes = "}".repeat(DEPTH);
        let expected = format!(
            "{{\n\"atoms\": [],\n\"formulas\": [\n{{\"formula\":{opens}{{\"atom\":\"EX-0001-001\"}}\
             {closes},\"origin\":{{\"requirement\":\"EX-0001-001\",\
             \"file\":\"rfcs/1-example/01-example.toml\",\"line\":1}},\"relation\":null}}\n],\n\
             \"conditions\": []\n}}\n"
        );
        assert_eq!(formulas_json(&set), expected);
    }
}
