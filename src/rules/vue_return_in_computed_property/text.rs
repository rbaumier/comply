//! vue-return-in-computed-property text backend.
//!
//! Comments are masked before scanning, so a commented-out `computed()` call
//! (and any `return` inside a comment) is ignored.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Walk from `start` (a `{` byte index in `src`) and return the byte index
/// of the matching `}`, ignoring braces inside strings and template
/// literals. Returns `None` if unbalanced.
fn matching_brace(src: &str, start: usize) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut depth: i32 = 0;
    let mut i = start;
    let mut in_str: Option<u8> = None;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' | b'\'' | b'`' => in_str = Some(c),
            b'{' => depth += 1,
            b'}' => {
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

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

/// Return `true` if the comment-masked block `body` contains a `return`
/// keyword at statement position.
///
/// In JavaScript `return` is always a statement, so any standalone `return`
/// keyword token implies the block returns. The keyword is recognised by the
/// bytes around it, so inline `case 'x': return y`, `if (c) return z`, and
/// `foo(); return w` all count as returns — not only a line-leading `return`.
///
/// A match requires both:
/// - the following byte is not an identifier byte (or the keyword ends the
///   body) — excludes identifiers such as `returnValue` / `returned`;
/// - the keyword is at statement position: it starts the body, follows a
///   statement boundary (`;`, `{`, `}`, `:`, `)`), or is line-leading (a
///   newline separates it from the previous token — ASI). Strings are not
///   masked, so requiring one of these keeps a `return` inside a single-line
///   string literal (`"return x"`, preceded by `"`) from counting, and also
///   excludes a `return` that is the tail of an identifier such as `preturn`
///   (preceding byte `p`).
fn has_statement_return(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut search_from = 0usize;
    while let Some(rel) = body[search_from..].find("return") {
        let p = search_from + rel;
        let after = p + "return".len();
        search_from = p + 1;
        if after < bytes.len() && is_ident_byte(bytes[after]) {
            continue;
        }
        match bytes[..p].iter().rposition(|b| !b.is_ascii_whitespace()) {
            None => return true,
            Some(q) => {
                if matches!(bytes[q], b';' | b'{' | b'}' | b':' | b')')
                    || bytes[q + 1..p].contains(&b'\n')
                {
                    return true;
                }
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Mask comments so a commented-out `computed()` is not matched and a
        // `return` inside a comment is not counted. `mask_comments` preserves
        // byte offsets and newlines, so computed line/column stay correct.
        let masked = crate::oxc_helpers::mask_comments(ctx.source);
        let src = masked.as_str();
        if !src.contains("computed(") {
            return Vec::new();
        }
        let mut diags = Vec::new();
        // Look for `computed(() => {` pattern — only block bodies can be
        // return-less. Arrow expression bodies always return.
        let needle = "computed(() => {";
        let mut cursor = 0usize;
        while let Some(rel) = src[cursor..].find(needle) {
            let abs = cursor + rel;
            let brace_idx = abs + needle.len() - 1; // the `{` byte
            let Some(end) = matching_brace(src, brace_idx) else { break };
            let body = &src[brace_idx + 1..end];
            if !has_statement_return(body) {
                // Compute line/column of the `computed(` keyword.
                let line_no = src[..abs].bytes().filter(|b| *b == b'\n').count() + 1;
                let col = src[..abs].rfind('\n').map(|nl| abs - nl).unwrap_or(abs + 1);
                diags.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: line_no,
                    column: col,
                    rule_id: super::META.id.into(),
                    message: "`computed()` callback has a block body but never returns — \
                              the property will resolve to `undefined`."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            cursor = end + 1;
        }
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("App.vue"), src))
    }

    #[test]
    fn flags_block_without_return() {
        let src = "const x = computed(() => { const a = 1; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_block_with_return() {
        let src = "const x = computed(() => { return 1; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_expression_body() {
        let src = "const x = computed(() => a.value + 1);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_commented_out_computed_line_comment() {
        // Regression #4427: the commented-out, return-less `computed()` must
        // not be flagged; only the real one below it is considered.
        let src = "// const x = computed(() => {\n//   return null\n// })\n\
                   const y = computed(() => { return 'ok' });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_commented_out_computed_block_comment() {
        let src = "/* const z = computed(() => {\n  doStuff()\n}) */";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_real_returnless_block() {
        let src = "const c = computed(() => { doStuff() });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_real_block_with_return() {
        let src = "const c = computed(() => { return 1 });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn double_slash_inside_string_is_not_a_comment() {
        // The `//` lives inside a string literal, so `mask_comments` leaves it
        // and the block is genuinely return-less → still flagged.
        let src = "const c = computed(() => { const u = \"a//b\"; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inline_case_returns() {
        // Regression #7569: an exhaustive `switch` whose every `case` ends in an
        // inline `return` returns on each branch and must not be flagged.
        let src = "const text = computed(() => {\n\
                   \tswitch (status) {\n\
                   \t\tcase 'online': return a;\n\
                   \t\tcase 'offline': return b;\n\
                   \t}\n\
                   });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inline_if_else_returns() {
        let src = "const v = computed(() => { if (c) return z; else return w; });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_semicolon_multiline_return() {
        // ASI style: `return` is line-leading, and the previous line ends in an
        // identifier (`value`), not a statement-boundary char. Line-leading
        // position is a statement position.
        let src = "const v = computed(() => {\n  const last = lastName.value\n  return last\n});";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_nested_arrow_and_inline_return() {
        // `=> { return ... }` and a trailing inline `; return ...` — neither is
        // at line-start position, both are statement-position returns.
        let src = "const v = computed(() => { const fn = () => { return 'x' }; return fn(); });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_returnless_block_with_multiple_statements() {
        // No `return` anywhere → resolves to `undefined` → still flagged.
        let src = "const v = computed(() => { doSideEffect(); logStuff(); });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_block_whose_only_return_is_in_a_string() {
        // The `return` lives inside a string literal (strings are not masked);
        // its preceding byte is `"`, not a statement boundary, so the block is
        // correctly seen as return-less.
        let src = "const v = computed(() => { const s = \"return x\"; });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_block_whose_only_return_is_in_a_comment() {
        // The `// return later` comment is masked away → return-less → flagged.
        let src = "const v = computed(() => { doStuff(); // return later\n });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_block_using_return_as_identifier_prefix() {
        // `returnValue` is an identifier, not a `return` statement.
        let src = "const v = computed(() => { const returnValue = 1; });";
        assert_eq!(run(src).len(), 1);
    }
}
