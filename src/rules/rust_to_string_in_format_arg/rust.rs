//! rust-to-string-in-format-arg backend.
//!
//! Walks every `macro_invocation` whose macro name is one of the
//! formatting macros (`format`, `println`, `print`, `eprintln`,
//! `eprint`, `write`, `writeln`, `format_args`) and inspects its
//! token tree for `.to_string()` calls. Each redundant `.to_string()`
//! call emits one diagnostic.
//!
//! We work off the macro's token-tree text (no inner AST) because
//! the grammar models macro arguments as opaque tokens.
//!
//! A `.to_string()` is only redundant when both hold:
//!
//! 1. Its result is the value the formatter consumes directly — i.e.
//!    the terminal value of a top-level macro argument. We skip a
//!    match when its result is fed somewhere else first: chained into
//!    another method (`.to_string().as_str()`, `.to_string().trim()`)
//!    or passed as an argument to a nested call
//!    (`indent(x.to_string(), ..)`).
//! 2. The placeholder consuming that argument is a `Display`
//!    placeholder (`{}`, `{:>width}`, …), not a `Debug` one
//!    (`{:?}`, `{:#?}`). With `{:?}`, `x.to_string()` formats the
//!    `Debug` of a `String` (a quoted string) while `x` formats the
//!    value's own `Debug` — different output, so the call is not
//!    redundant.
//!
//! The scan tracks parenthesis depth and skips string/char literal
//! contents so delimiters inside a format string don't corrupt it.
//! Mapping arguments to placeholders is positional: the Nth positional
//! placeholder consumes the Nth positional value argument. When the
//! mapping cannot be established confidently — the format string is not
//! a plain string literal, or it contains explicitly-indexed (`{0}`) or
//! named (`{name}`) placeholders, or the counts don't line up — we err
//! toward suppression and flag nothing.

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
        // calls. Only those the formatter consumes directly via a
        // `Display` placeholder are flagged.
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        for _ in 0..count_redundant_to_string(text, bare) {
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

/// Counts the `.to_string()` calls in `text` that are genuinely
/// redundant: the terminal value of a top-level macro argument that a
/// `Display` placeholder consumes.
///
/// `bare` is the macro name (`write`, `format`, …); for `write!` /
/// `writeln!` the first argument is the writer and is skipped before the
/// format string.
///
/// Returns 0 whenever the placeholder mapping cannot be established
/// confidently — see [`debug_positional_placeholders`].
fn count_redundant_to_string(text: &str, bare: &str) -> usize {
    let Some(body) = macro_body(text) else {
        return 0;
    };
    let args = split_top_level_args(body);

    // The writer occupies argument 0 for `write!` / `writeln!`.
    let fmt_idx = if matches!(bare, "write" | "writeln") {
        1
    } else {
        0
    };
    let Some(fmt_arg) = args.get(fmt_idx) else {
        return 0;
    };
    let Some(fmt_str) = string_literal_content(fmt_arg.trim()) else {
        // Format string is not a plain/raw string literal (e.g. a
        // `concat!` or constant): cannot map placeholders. Suppress.
        return 0;
    };
    let Some(positional_is_debug) = debug_positional_placeholders(&fmt_str) else {
        // Indexed/named placeholders make the positional mapping
        // ambiguous. Suppress.
        return 0;
    };

    let mut positional = 0usize;
    let mut count = 0usize;
    for arg in &args[fmt_idx + 1..] {
        // A named value argument (`name = expr`) is consumed by a named
        // placeholder, not a positional one, so it is excluded from the
        // positional count. We already suppressed everything if any named
        // placeholder exists, so such an argument never reaches a flag.
        if is_named_arg(arg) {
            continue;
        }
        let idx = positional;
        positional += 1;
        // Flag only when this argument's terminal `.to_string()` feeds a
        // `Display` placeholder. A `Debug` placeholder (`Some(true)`) or a
        // missing one (`None`, counts don't line up) is left alone.
        if arg_has_terminal_to_string(arg) && positional_is_debug.get(idx) == Some(&false) {
            count += 1;
        }
    }
    count
}

/// Returns the text between the macro's outer delimiter pair. `text` is
/// the whole invocation (`name!( .. )` / `name![ .. ]` / `name!{ .. }`);
/// we find the first delimiter after `!` and its match.
fn macro_body(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let open = bytes.iter().position(|&b| matches!(b, b'(' | b'[' | b'{'))?;
    let close = matching_close(bytes, open)?;
    text.get(open + 1..close)
}

/// Index of the delimiter closing the one opened at `open`, skipping
/// nested delimiters and string/char literal contents.
fn matching_close(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_string_literal(bytes, i);
                continue;
            }
            b'\'' if is_char_literal(bytes, i) => {
                i = skip_char_literal(bytes, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Splits a macro body into its top-level arguments (separated by commas
/// at depth 0 of the body), skipping commas inside nested delimiters and
/// string/char literals. A trailing comma yields no empty final argument.
fn split_top_level_args(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut args = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_string_literal(bytes, i);
                continue;
            }
            b'\'' if is_char_literal(bytes, i) => {
                i = skip_char_literal(bytes, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b',' if depth == 0 => {
                args.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let tail = body[start..].trim();
    if !tail.is_empty() {
        args.push(&body[start..]);
    }
    args
}

/// Whether an argument is a named formatting argument: a leading
/// identifier followed by a single `=` (not `==`). The receiver of a
/// `.to_string()` and equality operators inside an expression are not
/// confused because we only look at the leading identifier.
fn is_named_arg(arg: &str) -> bool {
    let arg = arg.trim_start();
    let bytes = arg.as_bytes();
    let mut i = 0;
    if bytes.first().is_none_or(|&b| !is_ident_start(b)) {
        return false;
    }
    while i < bytes.len() && is_ident_continue(bytes[i]) {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    bytes.get(i) == Some(&b'=') && bytes.get(i + 1) != Some(&b'=')
}

fn is_ident_start(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphabetic()
}

fn is_ident_continue(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Whether `arg` ends with a `.to_string()` that the formatter consumes
/// directly: scanning the argument top-level (depth 0 within the arg),
/// the `.to_string()` sits at depth 0 and nothing but whitespace follows.
fn arg_has_terminal_to_string(arg: &str) -> bool {
    const PATTERN: &str = ".to_string()";
    let bytes = arg.as_bytes();
    let mut depth: i32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = skip_string_literal(bytes, i);
                continue;
            }
            b'\'' if is_char_literal(bytes, i) => {
                i = skip_char_literal(bytes, i);
                continue;
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'.' if depth == 0 && arg[i..].starts_with(PATTERN) => {
                let after = i + PATTERN.len();
                if only_whitespace_after(bytes, after) {
                    return true;
                }
                i = after;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// If `arg` is exactly a plain (`"..."`) or raw (`r"..."` / `r#"..."#`)
/// string literal, returns its raw inner content (escapes left intact).
/// Returns `None` when the argument is anything else (a `concat!`, a
/// constant, an expression, a byte string, …).
fn string_literal_content(arg: &str) -> Option<String> {
    let bytes = arg.as_bytes();
    let open = bytes.iter().position(|&b| b == b'"')?;
    // Only a raw-string prefix (`r`, `r#`, `r##`, …) or nothing may
    // precede the opening quote. Anything else means the argument is not
    // a bare string literal.
    let prefix = &arg[..open];
    let is_raw = match prefix {
        "" => false,
        _ if prefix.starts_with('r') && prefix[1..].bytes().all(|b| b == b'#') => true,
        _ => return None,
    };
    let end = skip_string_literal(bytes, open);
    // The literal must span the entire argument.
    if end != bytes.len() {
        return None;
    }
    let hashes = prefix.bytes().filter(|&b| b == b'#').count();
    let inner_start = open + 1;
    let inner_end = end - 1 - if is_raw { hashes } else { 0 };
    arg.get(inner_start..inner_end).map(str::to_owned)
}

/// Parses the format string's placeholders left to right and returns, for
/// each *positional* (auto-numbered) placeholder, whether it is a `Debug`
/// placeholder (`{:?}`, `{:#?}`, `{:>10?}`, …).
///
/// Returns `None` — meaning "do not flag anything" — when the string
/// contains an explicitly-indexed (`{0}`) or named (`{name}`) placeholder,
/// since those break the simple Nth-positional-placeholder ↔
/// Nth-positional-argument mapping. Escaped braces `{{` / `}}` are not
/// placeholders.
fn debug_positional_placeholders(fmt: &str) -> Option<Vec<bool>> {
    let bytes = fmt.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' if bytes.get(i + 1) == Some(&b'{') => i += 2,
            b'}' if bytes.get(i + 1) == Some(&b'}') => i += 2,
            b'{' => {
                let close = i + 1 + bytes[i + 1..].iter().position(|&b| b == b'}')?;
                let inner = &fmt[i + 1..close];
                let (arg_ref, spec) = match inner.split_once(':') {
                    Some((a, s)) => (a.trim(), Some(s)),
                    None => (inner.trim(), None),
                };
                // An explicit argument reference (index or name) breaks
                // positional mapping.
                if !arg_ref.is_empty() {
                    return None;
                }
                out.push(spec.is_some_and(spec_is_debug));
                i = close + 1;
            }
            _ => i += 1,
        }
    }
    Some(out)
}

/// Whether a format spec selects the `Debug` trait, i.e. its `type`
/// component is `?` / `x?` / `X?`. A leading `fill`+`align` (e.g. `?<` in
/// `{:?<5}`) is stripped first so a `?` used as the fill character is not
/// mistaken for the `Debug` type, which always terminates the spec.
fn spec_is_debug(spec: &str) -> bool {
    let chars: Vec<char> = spec.chars().collect();
    let rest = if chars.len() >= 2 && is_align(chars[1]) {
        &chars[2..]
    } else if chars.first().is_some_and(|&c| is_align(c)) {
        &chars[1..]
    } else {
        &chars[..]
    };
    rest.last() == Some(&'?')
}

fn is_align(c: char) -> bool {
    matches!(c, '<' | '^' | '>')
}

/// True when only whitespace follows a `.to_string()` match within its
/// argument — meaning the call is the argument's terminal value. A
/// trailing `.` (or any other token) means the result is chained into
/// another method, so it is not consumed directly.
fn only_whitespace_after(bytes: &[u8], after: usize) -> bool {
    bytes[after..].iter().all(u8::is_ascii_whitespace)
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

    #[test]
    fn allows_to_string_feeding_debug_placeholder() {
        // `{:?}` formats the `Debug` of the `String` (a quoted string),
        // which differs from the value's own `Debug`. Not redundant.
        let source = "fn f(f: &mut String, v: u8) { write!(f, \"x {:?}\", v.to_string()).unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_to_string_feeding_pretty_debug_placeholder() {
        // `{:#?}` (pretty Debug) is also a Debug placeholder.
        let source = "fn f(x: u8) { let _ = format!(\"{:#?}\", x.to_string()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_serde_json_debug_first_arg() {
        // The serde_json repro: the first positional argument maps to the
        // first positional placeholder `{:?}` (Debug); `line`/`column`
        // carry no `.to_string()`.
        let source = "fn f(f: &mut String, code: u8, line: u8, column: u8) { write!(f, \"Error({:?}, line: {}, column: {})\", code.to_string(), line, column).unwrap(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_only_display_in_mixed_placeholders() {
        // `a.to_string()` feeds `{}` (Display, redundant); `b.to_string()`
        // feeds `{:?}` (Debug, kept). Exactly one diagnostic, on `a`.
        let source = "fn f(a: u8, b: u8) { let _ = format!(\"{} {:?}\", a.to_string(), b.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn suppresses_when_format_string_is_not_a_literal() {
        // The format string is a `concat!`, not a plain literal: the
        // placeholder mapping is unknowable. Suppress conservatively.
        let source =
            "fn f(x: u8) { let _ = format!(concat!(\"{}\"), x.to_string()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn suppresses_when_indexed_placeholder_present() {
        // `{0}` is an explicit positional index; the positional mapping is
        // ambiguous. Suppress conservatively, even for a Display use.
        let source = "fn f(x: u8) { let _ = format!(\"{0}\", x.to_string()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn suppresses_when_named_placeholder_present() {
        // A named placeholder `{val}` breaks positional mapping.
        let source = "fn f(x: u8) { let _ = format!(\"{val}\", val = x.to_string()); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_display_with_width_spec() {
        // `{:>8}` is a Display placeholder with alignment/width — still
        // redundant.
        let source = "fn f(x: u8) { let _ = format!(\"{:>8}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn ignores_escaped_braces() {
        // `{{}}` are literal braces, not a placeholder: the single real
        // placeholder `{}` consumes the only argument.
        let source = "fn f(x: u8) { let _ = format!(\"{{}} {}\", x.to_string()); }";
        assert_eq!(run_on(source).len(), 1);
    }
}
