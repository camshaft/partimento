//! Reading a file of the tree as TOML, with a finding at the line of each error.

use serde::de::DeserializeOwned;

use crate::{ByteVec, Finding};

/// The rule of a file that isn't UTF-8 text or doesn't parse.
pub const RULE: &str = "parse";

/// The offset of each newline of a file, read in one pass over its bytes, so that the line of
/// each span of the file costs a binary search and not a scan from the file's start.
pub(crate) struct Lines {
    newlines: Vec<usize>,
}

impl Lines {
    /// Reads the offset of each newline of `bytes`.
    pub(crate) fn new(bytes: &[u8]) -> Lines {
        let newlines = bytes
            .iter()
            .enumerate()
            .filter(|&(_, &byte)| byte == b'\n')
            .map(|(offset, _)| offset)
            .collect();
        Lines { newlines }
    }

    /// The line, counted from 1, that holds the byte at `offset`: one more than the newlines
    /// before it, so that an offset past the end of the file counts every newline of the file.
    pub(crate) fn line(&self, offset: usize) -> u32 {
        let newlines = self.newlines.partition_point(|&at| at < offset);
        line_after(newlines)
    }
}

/// The line, counted from 1, that follows `newlines` newlines, or `u32::MAX` where the count
/// passes it.
fn line_after(newlines: usize) -> u32 {
    let newlines = u32::try_from(newlines).unwrap_or(u32::MAX);
    newlines.saturating_add(1)
}

/// The file's text, or a finding where it isn't UTF-8.
pub(crate) fn text(file: &str, bytes: &ByteVec, findings: &mut Vec<Finding>) -> Option<String> {
    let bytes = bytes.copy_to_bytes().to_vec();
    match String::from_utf8(bytes) {
        Ok(text) => Some(text),
        Err(error) => {
            let offset = error.utf8_error().valid_up_to();
            let line = Lines::new(error.as_bytes()).line(offset);
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
            let line = Lines::new(text.as_bytes()).line(start);
            let message = error.message().trim_end();
            findings.push(Finding::error(RULE, file, line, None, message));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{line_after, parse, text, Lines};
    use crate::ByteVec;

    /// A text of four lines, `a`, `bc`, an empty line and `d`, whose newlines are at 1, 4 and 5.
    const TEXT: &[u8] = b"a\nbc\n\nd";

    #[test]
    fn each_byte_is_on_the_line_that_holds_it() {
        let lines = Lines::new(TEXT);
        let expected: [u32; 7] = [1, 1, 2, 2, 2, 3, 4];
        for (offset, line) in expected.into_iter().enumerate() {
            assert_eq!(lines.line(offset), line, "offset {offset}");
        }
    }

    #[test]
    fn an_offset_at_or_past_the_end_is_on_the_last_line() {
        let lines = Lines::new(TEXT);
        for offset in [TEXT.len(), TEXT.len() + 1, usize::MAX] {
            assert_eq!(lines.line(offset), 4, "offset {offset}");
        }
    }

    #[test]
    fn a_text_without_a_newline_is_one_line() {
        assert_eq!(Lines::new(b"").line(0), 1);
        assert_eq!(Lines::new(b"abc").line(2), 1);
    }

    #[test]
    fn the_line_is_one_more_than_the_newlines_before_the_offset() {
        let text = b"x = 1\n\n[t]\ny = 2\n";
        let lines = Lines::new(text);
        for offset in 0..=text.len() + 1 {
            let end = offset.min(text.len());
            let before = text[..end].iter().filter(|&&byte| byte == b'\n').count();
            let expected = u32::try_from(before).expect("the count fits") + 1;
            assert_eq!(lines.line(offset), expected, "offset {offset}");
        }
    }

    #[test]
    fn a_count_past_u32_ends_at_the_last_line_a_u32_counts() {
        assert_eq!(line_after(0), 1);
        let max = usize::try_from(u32::MAX).expect("u32 fits");
        assert_eq!(line_after(max - 1), u32::MAX);
        assert_eq!(line_after(max), u32::MAX);
        assert_eq!(line_after(usize::MAX), u32::MAX);
    }

    #[test]
    fn a_file_that_isnt_utf8_has_its_finding_at_the_line_of_its_first_bad_byte() {
        let bytes = ByteVec::from(b"a = 1\nb = \xff\n".to_vec());
        let mut findings = Vec::new();
        assert!(text("f.toml", &bytes, &mut findings).is_none());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
    }

    #[test]
    fn a_parse_error_has_its_finding_at_the_line_of_the_error() {
        let source = "a = 1\nb = 2\nc = \n";
        let mut findings = Vec::new();
        let parsed: Option<toml::Table> = parse("f.toml", source, &mut findings);
        assert!(parsed.is_none());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 3);
    }
}
