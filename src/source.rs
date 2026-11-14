//! Reading a file of the tree as TOML, with a finding at the line of each error.

use serde::de::DeserializeOwned;

use crate::{ByteVec, Finding};

/// The rule of a file that isn't UTF-8 text or doesn't parse.
pub const RULE: &str = "parse";

/// The line, counted from 1, that holds the byte at `offset`.
pub(crate) fn line_of(bytes: &[u8], offset: usize) -> u32 {
    let end = offset.min(bytes.len());
    let newlines = bytes[..end].iter().filter(|&&byte| byte == b'\n').count();
    let newlines = u32::try_from(newlines).unwrap_or(u32::MAX);
    newlines.saturating_add(1)
}

/// The file's text, or a finding where it isn't UTF-8.
pub(crate) fn text(file: &str, bytes: &ByteVec, findings: &mut Vec<Finding>) -> Option<String> {
    let bytes = bytes.copy_to_bytes().to_vec();
    match String::from_utf8(bytes) {
        Ok(text) => Some(text),
        Err(error) => {
            let line = line_of(error.as_bytes(), error.utf8_error().valid_up_to());
            let message = "the file isn't UTF-8 text";
            findings.push(Finding::error(RULE, file, line, None, message));
            None
        }
    }
}

/// The file's text parsed as TOML into `T`, or a finding at the line of the parser's error.
pub(crate) fn parse<T: DeserializeOwned>(
    file: &str,
    text: &str,
    findings: &mut Vec<Finding>,
) -> Option<T> {
    match toml::from_str(text) {
        Ok(value) => Some(value),
        Err(error) => {
            let start = error.span().map_or(0, |span| span.start);
            let line = line_of(text.as_bytes(), start);
            let message = error.message().trim_end();
            findings.push(Finding::error(RULE, file, line, None, message));
            None
        }
    }
}
