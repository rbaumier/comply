//! rust-to-string-in-format-arg backend.
//!
//! Walks every `macro_invocation` whose macro name is one of the
//! formatting macros (`format`, `println`, `print`, `eprintln`,
//! `eprint`, `write`, `writeln`, `format_args`) and inspects its
//! token tree for `.to_string()` calls. Each `.to_string()` call
//! emits one diagnostic.
//!
//! We work off the macro's token-tree text (no inner AST) because
//! the grammar models macro arguments as opaque tokens.
//!
//! A `.to_string()` is only redundant when its result is the value
//! the formatter consumes directly — i.e. the terminal value of a
//! top-level macro argument. We skip a match when its result is fed
//! somewhere else first: chained into another method
//! (`.to_string().as_str()`, `.to_string().trim()`) or passed as an
//! argument to a nested call (`indent(x.to_string(), ..)`). The scan
//! tracks parenthesis depth and skips string/char literal contents so
//! delimiters inside a format string don't corrupt it.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["macro_invocation"];

const FORMAT_MACROS: &[&str] = &[
    "format",
    "println",
    "print",
    "eprintln",
    "eprint",
    "write",
    "writeln",
    "format_args",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(macro_name) = node.child_by_field_name("macro") else {
            return;
        };
        let name = macro_name.utf8_text(source_bytes).unwrap_or("");
        let bare = name.rsplit("::").next().unwrap_or(name);
        if !FORMAT_MACROS.contains(&bare) {
            return;
        }
        // Scan the macro's token-tree text for redundant `.to_string()`
        // calls. Only those whose result the formatter consumes directly
        // are flagged.
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        for _ in find_redundant_to_string(text) {
            diagnostics.push(Diagnostic::at_node(
                std::sync::Arc::clone(&ctx.path_arc),
                &node,
                "rust-to-string-in-format-arg",
                format!(
                    "`.to_string()` inside `{bare}!(..)` is redundant — \
                     the formatter already calls `Display`. Drop the call."
                ),
                Severity::Warning,
            ));
        }
    }
}

/// Returns the byte offsets of each `.to_string()` whose result the
/// formatter consumes directly: the terminal value of a top-level macro
/// argument. A match is skipped when its result is fed elsewhere first —
/// chained into another method (`.to_string().as_str()`) or passed as an
/// argument to a nested call (`indent(x.to_string(), ..)`).
///
/// The scan tracks parenthesis depth and skips string/char literal
/// contents so delimiters inside a format string never corrupt it. The
/// macro's own outer `(` puts arguments at depth 1, the top argument
/// level.
fn find_redundant_to_string(text: &str) -> Vec<usize> {
    const PATTERN: &str = ".to_string()";
    const TOP_ARG_DEPTH: i32 = 1;

    let bytes = text.as_bytes();
    let mut hits = Vec::new();
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                // Raw string `r"..."` / `r#"..."#` or a plain `"..."`.
                i = skip_string_literal(bytes, i);
                continue;
            }
            b'\'' if is_char_literal(bytes, i) => {
                i = skip_char_literal(bytes, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'.' if text[i..].starts_with(PATTERN) => {
                // `depth` here is the level of the receiver expression;
                // the macro's outer `(` makes top arguments depth 1.
                let after = i + PATTERN.len();
                if depth == TOP_ARG_DEPTH && consumed_directly(bytes, after) {
                    hits.push(i);
                }
                i = after;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    hits
}

/// True when the next significant byte after a `.to_string()` match is a
/// format-argument delimiter (`,` or `)`) — meaning the call is the
/// terminal value of the argument. A leading `.` means the result is
/// chained into another method, so it is not consumed directly.
fn consumed_directly(bytes: &[u8], after: usize) -> bool {
    let mut i = after;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    matches!(bytes.get(i), Some(b',') | Some(b')'))
}

/// Advances past a string literal starting at the opening `"` at `start`.
/// Detects raw strings (`r"..."` / `r#"..."#`) by walking back over the
/// `#`s and the `r` prefix: in a raw string backslashes do not escape and
/// the literal ends at `"` followed by the same number of `#`s. In a
/// plain string, `\"` is an escaped quote.
fn skip_string_literal(bytes: &[u8], start: usize) -> usize {
    let mut hashes = 0;
    let mut j = start;
    while j > 0 && bytes[j - 1] == b'#' {
        j -= 1;
        hashes += 1;
    }
    let is_raw = j > 0 && bytes[j - 1] == b'r';
    let hashes = if is_raw { hashes } else { 0 };
    let mut i = start + 1;
    if is_raw {
        while i < bytes.len() {
            if bytes[i] == b'"' && closing_hashes_match(bytes, i + 1, hashes) {
                return i + 1 + hashes;
            }
            i += 1;
        }
    } else {
        while i < bytes.len() {
            match bytes[i] {
                b'\\' => i += 2,
                b'"' => return i + 1,
                _ => i += 1,
            }
        }
    }
    i
}

fn closing_hashes_match(bytes: &[u8], at: usize, hashes: usize) -> bool {
    (0..hashes).all(|k| bytes.get(at + k) == Some(&b'#'))
}

/// Distinguishes a char literal `'c'` / `'\n'` / lifetime tick. A char
/// literal has a closing `'` within a few bytes; a lifetime (`'a`) does
/// not, so we conservatively require a closing quote.
fn is_char_literal(bytes: &[u8], start: usize) -> bool {
    // `'\X'` or `'X'` — closing quote within 4 bytes accounts for escapes.
    let mut i = start + 1;
    if bytes.get(i) == Some(&b'\\') {
        i += 1;
    }
    i += 1;
    bytes.get(i) == Some(&b'\'')
}

fn skip_char_literal(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    if bytes.get(i) == Some(&b'\\') {
        i += 2;
    } else {
        i += 1;
    }
    // Now at the closing quote.
    i + 1
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_format_with_to_string() {
        let source = "fn f(x: u8) { let _ = format!(\"{}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_println_with_to_string() {
        let source = "fn f(x: u8) { println!(\"{}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_writeln_with_to_string() {
        let source = "fn f(w: &mut String, x: u8) { writeln!(w, \"{}\", x.to_string()).unwrap(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_format_without_to_string() {
        let source = "fn f(x: u8) { let _ = format!(\"{}\", x); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_to_string_outside_format() {
        let source = "fn f(x: u8) { let _ = x.to_string(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_to_string_chained_into_trim() {
        // `.to_string().trim()` — the `{}` formats the trimmed `&str`,
        // not the value; dropping `.to_string()` would not compile.
        let source =
            "fn f(f: &mut String, source: u8) { writeln!(f, \"Caused by: {}\", source.to_string().trim()).unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_to_string_chained_into_as_str_in_nested_call() {
        // `.to_string().as_str()` fed into `indent(..)`; the `{}` formats
        // the `indent` result.
        let source = "fn f(f: &mut String, reason: u8) { write!(f, \"{}\", textwrap::indent(reason.to_string().as_str(), \"  \")).unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_to_string_as_nested_call_argument() {
        // `x.to_string()` is an argument to a nested call, not a top-level
        // macro argument value.
        let source =
            "fn f(f: &mut String, x: u8) { write!(f, \"{}\", indent(x.to_string(), \"  \")).unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_terminal_to_string_despite_punctuation_in_format_string() {
        // The format string literal contains `.`, `(`, `,` — the scan must
        // skip literal contents and still flag the terminal `x.to_string()`.
        let source = "fn f(x: u8) { let _ = format!(\"a.(b), {}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_terminal_to_string_with_raw_string_format() {
        // A raw string with embedded `"` and `(` must not desync the scan.
        let source = "fn f(x: u8) { let _ = format!(r#\"a.(b)\"c, {}\"#, x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }
}
