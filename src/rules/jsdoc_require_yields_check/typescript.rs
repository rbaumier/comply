//! jsdoc/require-yields-check — `@yields` tag matches actual `yield` usage.
//!
//! Flags two mismatches:
//!   - `@yields` present but the attached function has no `yield`.
//!   - Function has `yield` but no `@yields` tag (complements
//!     `require-yields`, kept narrow: we only flag when a JSDoc block already
//!     exists).
//!
//! Heuristic: inspect up to ~40 lines after the JSDoc block to detect
//! `yield` in the function body.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, has_tag, parse_tags};

fn function_body_after(source: &str, block_raw: &str) -> String {
    let idx = match source.find(block_raw) {
        Some(i) => i + block_raw.len(),
        None => return String::new(),
    };
    let tail = &source[idx..];
    let mut out = String::new();
    let mut lines = 0;
    for line in tail.lines() {
        out.push_str(line);
        out.push('\n');
        lines += 1;
        if lines >= 40 {
            break;
        }
    }
    out
}

fn body_has_yield(code: &str) -> bool {
    // Match `yield ` or `yield*` or line-start `yield;`. Avoid matching
    // substring hits inside identifiers (`abcyield`) via word-boundary check.
    code.split_whitespace().any(|tok| {
        tok == "yield" || tok == "yield;" || tok == "yield*" || tok.starts_with("yield(")
    }) || code.contains(" yield ")
        || code.contains("\tyield ")
        || code.contains("\nyield ")
}

fn is_generator_signature(code: &str) -> bool {
    code.contains("function*") || code.contains("function *")
}

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in find_jsdoc_blocks(text) {
        let tags = parse_tags(&block.content);
        let has_yields_tag = has_tag(&tags, "yields");
        let body = function_body_after(ctx.source, text);
        let is_gen = is_generator_signature(&body);
        let yields_in_body = body_has_yield(&body);

        if has_yields_tag && !yields_in_body {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: block.start_line + 1 + line_offset,
                column: 1,
                rule_id: "jsdoc/require-yields-check".into(),
                message: "`@yields` is documented but the function does not yield — remove the tag.".into(),
                severity: Severity::Warning,
                span: None,
            });
        } else if is_gen && yields_in_body && !has_yields_tag {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: block.start_line + 1 + line_offset,
                column: 1,
                rule_id: "jsdoc/require-yields-check".into(),
                message: "Function yields but JSDoc is missing `@yields` — document what it yields.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_yields_tag_without_actual_yield() {
        let src = "/**\n * ok\n * @yields {number}\n */\nfunction* g() { return 1; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_generator_with_yield_but_no_yields_tag() {
        let src = "/**\n * ok\n */\nfunction* g() { yield 1; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_matched_yields_tag_and_yield() {
        let src = "/**\n * ok\n * @yields {number}\n */\nfunction* g() { yield 1; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_regular_function_without_tag() {
        let src = "/**\n * ok\n */\nfunction f() { return 1; }";
        assert!(run(src).is_empty());
    }
}
