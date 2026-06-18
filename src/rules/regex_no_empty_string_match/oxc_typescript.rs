//! regex-no-empty-string-match OXC backend.
//!
//! Flags regex literals passed to `.split()` or `.replace()` whose
//! pattern can match the empty string.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

// === nullability analysis ===
// A regex matches the empty string iff it is "nullable": some top-level
// alternative is nullable, and an alternative (a concatenation of terms) is
// nullable iff every one of its terms is nullable. A single mandatory term
// (a literal/class/`.` that must consume a character) makes the whole
// alternative non-nullable, so `\n\s*` does NOT match empty.
//
// Direction is conservative toward NOT flagging: any malformed or
// unrecognized construct is treated as a mandatory (non-nullable) atom, so
// uncertainty under-flags rather than over-flags.
fn pattern_can_match_empty(pattern: &str) -> bool {
    if is_fully_anchored(pattern) {
        return false;
    }
    alternatives_nullable(pattern)
}

fn is_fully_anchored(pattern: &str) -> bool {
    pattern.starts_with('^') && pattern.ends_with('$')
}

/// Nullable iff ANY top-level alternative (split on `|` at depth 0) is nullable.
/// An empty alternative (e.g. the right side of `a|`) is nullable.
fn alternatives_nullable(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut depth_paren: i32 = 0;
    let mut in_class = false;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'\\' {
            i += 2;
            continue;
        }
        if in_class {
            if c == b']' {
                in_class = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'[' => in_class = true,
            b'(' => depth_paren += 1,
            b')' => depth_paren -= 1,
            b'|' if depth_paren == 0 => {
                if concat_nullable(&pattern[start..i]) {
                    return true;
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    concat_nullable(&pattern[start..])
}

/// A concatenation is nullable iff every term is nullable.
fn concat_nullable(concat: &str) -> bool {
    let bytes = concat.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let (atom_end, atom_kind) = parse_atom(bytes, i);
        if atom_end == i {
            // Could not make progress (malformed) — treat as mandatory.
            return false;
        }
        let (next, quant_min_zero) = parse_quantifier(bytes, atom_end);
        if !term_nullable(&concat[i..atom_end], atom_kind, quant_min_zero) {
            return false;
        }
        i = next;
    }
    true
}

/// The kinds of atom relevant to nullability.
#[derive(Clone, Copy)]
enum AtomKind {
    /// Anchor (`^`, `$`, `\b`, `\B`) or lookaround group — consumes nothing.
    ZeroWidth,
    /// A capturing/non-capturing/named group `(...)` carrying its inner text.
    Group,
    /// A literal char, `.`, escape class, or `[...]` class — consumes ≥1 char.
    Consuming,
}

/// Whether a term (atom + optional quantifier) is nullable.
fn term_nullable(atom_text: &str, kind: AtomKind, quant_min_zero: bool) -> bool {
    match kind {
        AtomKind::ZeroWidth => true,
        AtomKind::Group => {
            if quant_min_zero {
                return true;
            }
            // Quantifier none / `+` / `{1,…}`: nullable iff the inner is.
            alternatives_nullable(group_inner(atom_text))
        }
        AtomKind::Consuming => quant_min_zero,
    }
}

/// Strip the group delimiters and any prefix (`?:`, `?=`, `?!`, `?<=`, `?<!`,
/// `?<name>`) to expose the inner pattern of a group atom.
fn group_inner(atom_text: &str) -> &str {
    let inner = atom_text
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or("");
    if let Some(rest) = inner.strip_prefix("?:") {
        return rest;
    }
    if let Some(rest) = inner.strip_prefix("?<") {
        // Named group `?<name>...`; lookbehinds are handled as zero-width atoms.
        if let Some(idx) = rest.find('>') {
            return &rest[idx + 1..];
        }
        return "";
    }
    if inner.starts_with('?') {
        // Any other `(?...)` form is a lookaround (zero-width), not reached here.
        return "";
    }
    inner
}

/// Parse one atom starting at `start`. Returns `(end, kind)` where `end` is the
/// byte index just past the atom; `end == start` signals no progress.
fn parse_atom(bytes: &[u8], start: usize) -> (usize, AtomKind) {
    if start >= bytes.len() {
        return (start, AtomKind::Consuming);
    }
    match bytes[start] {
        b'^' | b'$' => (start + 1, AtomKind::ZeroWidth),
        b'\\' => {
            if start + 1 >= bytes.len() {
                // Trailing backslash — malformed; no progress.
                return (start, AtomKind::Consuming);
            }
            let next = bytes[start + 1];
            let kind = if next == b'b' || next == b'B' {
                AtomKind::ZeroWidth
            } else {
                AtomKind::Consuming
            };
            // Skip the `\` plus the full escaped char (a class like `\d` is one
            // ASCII byte; a literal escape like `\é` may be multi-byte).
            (start + 1 + utf8_char_len(next), kind)
        }
        b'[' => {
            let end = char_class_end(bytes, start);
            (end, AtomKind::Consuming)
        }
        b'(' => {
            let end = group_end(bytes, start);
            if end == start {
                return (start, AtomKind::Consuming);
            }
            let kind = if is_lookaround(bytes, start) {
                AtomKind::ZeroWidth
            } else {
                AtomKind::Group
            };
            (end, kind)
        }
        // A literal char. Advance by the full UTF-8 width so the slice in
        // `concat_nullable` never lands mid-codepoint.
        _ => (start + utf8_char_len(bytes[start]), AtomKind::Consuming),
    }
}

/// Byte width of a UTF-8 sequence from its leading byte. A continuation or
/// invalid lead byte counts as one byte (the bytes we walk come from a valid
/// `&str`, so this only ever short-circuits defensively).
fn utf8_char_len(lead: u8) -> usize {
    match lead {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

/// Whether a group opening at `start` is a lookaround (`(?=`, `(?!`, `(?<=`,
/// `(?<!`).
fn is_lookaround(bytes: &[u8], start: usize) -> bool {
    if bytes.get(start + 1) != Some(&b'?') {
        return false;
    }
    match bytes.get(start + 2) {
        Some(b'=') | Some(b'!') => true,
        Some(b'<') => matches!(bytes.get(start + 3), Some(b'=') | Some(b'!')),
        _ => false,
    }
}

/// Index just past the closing `]` of a char class opened at `start`, honoring
/// `\]`. Returns `bytes.len()` if unterminated.
fn char_class_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b']' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Index just past the matching `)` of a group opened at `start`, tracking
/// nesting and skipping escapes and char classes. Returns `start` (no progress)
/// if unbalanced.
fn group_end(bytes: &[u8], start: usize) -> usize {
    let mut depth: i32 = 0;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'[' => {
                i = char_class_end(bytes, i);
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    start
}

/// Parse an optional quantifier at `start` (`*`, `+`, `?`, `{m,n}`) plus a
/// trailing lazy `?`. Returns `(end, min_is_zero)` where `min_is_zero` is true
/// when the quantifier permits zero repetitions.
fn parse_quantifier(bytes: &[u8], start: usize) -> (usize, bool) {
    if start >= bytes.len() {
        return (start, false);
    }
    let (mut end, min_zero) = match bytes[start] {
        b'*' | b'?' => (start + 1, true),
        b'+' => (start + 1, false),
        b'{' => match brace_quantifier(bytes, start) {
            Some((brace_end, min_zero)) => (brace_end, min_zero),
            None => return (start, false),
        },
        _ => return (start, false),
    };
    // Optional lazy marker.
    if bytes.get(end) == Some(&b'?') {
        end += 1;
    }
    (end, min_zero)
}

/// Parse `{m}` / `{m,} `/ `{m,n}` at `start`. Returns `(end, min_is_zero)` or
/// `None` if it is not a well-formed brace quantifier.
fn brace_quantifier(bytes: &[u8], start: usize) -> Option<(usize, bool)> {
    let mut i = start + 1;
    let min_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == min_start {
        return None; // `{` with no leading number — not a quantifier.
    }
    let min_is_zero = bytes[min_start..i].iter().all(|&b| b == b'0');
    if bytes.get(i) == Some(&b',') {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if bytes.get(i) == Some(&b'}') {
        Some((i + 1, min_is_zero))
    } else {
        None
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else { return };
        let pattern = re.regex.pattern.text.as_str();
        if !pattern_can_match_empty(pattern) {
            return;
        }
        // Walk up to check if this regex is an argument of .split() or .replace().
        if !is_arg_of_split_or_replace(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "regex-no-empty-string-match".into(),
            message: "Regex can match the empty string in `.split()` or `.replace()` \u{2014} this may cause unexpected results.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_arg_of_split_or_replace<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    let mut cur_id = nodes.parent_id(node.id());
    loop {
        if cur_id == node.id() || cur_id == nodes.parent_id(cur_id) {
            return false;
        }
        let parent_kind = nodes.kind(cur_id);
        if let AstKind::CallExpression(call) = parent_kind {
            if let Expression::StaticMemberExpression(member) = &call.callee {
                let name = member.property.name.as_str();
                return name == "split" || name == "replace";
            }
            return false;
        }
        cur_id = nodes.parent_id(cur_id);
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- True positives (genuine optional / nullable patterns). ---

    #[test]
    fn flags_replace_with_optional() {
        assert_eq!(run_on(r#"const r = s.replace(/a?/g, 'x');"#).len(), 1);
    }

    #[test]
    fn flags_split_with_star() {
        assert_eq!(run_on(r#"const r = s.split(/x*/);"#).len(), 1);
    }

    #[test]
    fn flags_lone_nullable_class() {
        assert_eq!(run_on(r#"const r = s.replace(/\s*/g, '');"#).len(), 1);
    }

    #[test]
    fn flags_alternation_with_nullable_branch() {
        assert_eq!(run_on(r#"const r = s.replace(/a|b?/g, '');"#).len(), 1);
    }

    #[test]
    fn flags_nullable_group_star() {
        assert_eq!(run_on(r#"const r = s.replace(/(foo)*/g, '');"#).len(), 1);
    }

    #[test]
    fn flags_nullable_group_optional() {
        assert_eq!(run_on(r#"const r = s.replace(/(abc)?/g, '');"#).len(), 1);
    }

    // --- #3783: a mandatory atom next to a nullable quantifier is NOT empty. ---

    #[test]
    fn allows_mandatory_literal_then_nullable() {
        assert!(run_on(r#"const collapse = (s) => s.replace(/\n\s*/g, '');"#).is_empty());
    }

    #[test]
    fn allows_literal_then_star() {
        assert!(run_on(r#"const r = s.replace(/a\s*/g, '');"#).is_empty());
    }

    #[test]
    fn allows_plus_quantifier() {
        assert!(run_on(r#"const r = s.split(/x+/);"#).is_empty());
    }

    #[test]
    fn allows_all_literal() {
        assert!(run_on(r#"const r = s.replace(/foo/g, '');"#).is_empty());
    }

    #[test]
    fn handles_multibyte_mandatory_literal_without_panic() {
        // Mandatory multi-byte literal then a nullable quantifier — non-nullable,
        // and must not panic on the UTF-8 boundary.
        assert!(run_on(r#"const r = s.replace(/é\s*/g, '');"#).is_empty());
    }

    // --- #3775: lookaround group prefixes must not be read as optional. ---

    #[test]
    fn allows_lookahead_group_prefix() {
        assert!(run_on(r#"const grouped = (s) => s.replace(/(\d)(?=(\d\d\d)+(?!\d))/g, '$1,');"#).is_empty());
    }

    #[test]
    fn allows_alternation_with_negative_lookahead() {
        assert!(run_on(r#"const cased = (d) => d.replace(/[A-Z]+(?![a-z])|[A-Z]/g, (m) => m);"#).is_empty());
    }

    #[test]
    fn allows_positive_lookbehind() {
        assert!(run_on(r#"const r = s.replace(/(?<=\d)x/g, '');"#).is_empty());
    }

    #[test]
    fn allows_negative_lookbehind() {
        assert!(run_on(r#"const r = s.replace(/(?<!\d)x/g, '');"#).is_empty());
    }
}
