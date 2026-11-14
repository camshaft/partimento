//! The `partimento` command: reads specification trees from disk and runs the library on them.
//!
//! `partimento check [--root <dir>] [--format text|json] [--final]` prints each finding and exits
//! with 0 where no finding fails the check, 1 where one does, and 2 where the arguments are wrong
//! or a file can't be read.
//!
//! `partimento diff <base> <head> [--format text|json]` prints each requirement added, changed or
//! retired from the tree at `<base>` to the tree at `<head>` and exits with 0. Where either tree
//! fails a format rule, it prints that tree's failing findings instead and exits with 1, and it
//! exits with 2 where the arguments are wrong or a file can't be read.

use std::collections::BTreeMap;
use std::fmt::{Display, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

use partimento::{config, ByteVec, Change, Finding, Options, Tree};

const USAGE: &str = concat!(
    "usage: partimento check [--root <dir>] [--format text|json] [--final]\n",
    "       partimento diff <base> <head> [--format text|json]",
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Json,
}

enum Command {
    /// `check`, of the tree at a root.
    Check(PathBuf, Options),
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
        Command::Check(root, options) => {
            let report = partimento::check_default(&read_tree(root)?, options);
            (findings_output(&report.findings, args.format), report.failed())
        }
        Command::Diff(base, head) => {
            let (base, head) = (read_tree(base)?, read_tree(head)?);
            match partimento::diff(&base, &head) {
                Ok(changes) => (changes_output(&changes, args.format), false),
                Err(findings) => (findings_output(&findings, args.format), true),
            }
        }
    };
    print!("{output}");
    Ok(ExitCode::from(u8::from(failed)))
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    match args.next().as_deref() {
        Some("check") => parse_check(args),
        Some("diff") => parse_diff(args),
        Some(command) => Err(format!("`{command}` isn't a command")),
        None => Err("the command is missing".to_string()),
    }
}

fn parse_check(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut root = PathBuf::from(".");
    let mut format = Format::Text;
    let mut options = Options::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                let dir = args.next().ok_or("--root needs a directory")?;
                root = PathBuf::from(dir);
            }
            "--format" => format = parse_format(args.next())?,
            "--final" => options.final_pass = true,
            _ => return Err(format!("`{arg}` isn't an argument of `check`")),
        }
    }
    let command = Command::Check(root, options);
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

/// Reads spec.toml and each file under the set's directories that spec.toml lists.
fn read_tree(root: &Path) -> Result<Tree, String> {
    let mut tree = Tree::default();
    let spec = root.join(config::FILE);
    if is_link(&spec) {
        return Err(link(&spec));
    }
    let bytes = fs::read(&spec).map_err(|error| unreadable(&spec, &error))?;
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
        Err(error) => return Err(unreadable(&path, &error)),
    };
    let failed = |error: io::Error| unreadable(&path, &error);
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
            let bytes = fs::read(&file).map_err(|error| unreadable(&file, &error))?;
            files.insert(relative, ByteVec::from(bytes));
        }
    }
    Ok(())
}

fn unreadable(path: &Path, error: &io::Error) -> String {
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
    if items.is_empty() {
        return "[]\n".to_string();
    }
    format!("[\n{}\n]\n", items.join(",\n"))
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
