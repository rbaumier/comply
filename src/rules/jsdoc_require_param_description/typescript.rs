//! jsdoc/require-param-description — flag `@param` tags that carry
//! only a type + name (no description).
//!
//! A type-only `@param` is redundant with the TypeScript signature;
//! the real value of JSDoc params is the description that explains
//! intent ("the user id" / "rounded to the nearest cent"). Without
//! it the block is just noise.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_helpers::scan_blocks;

crate::ast_check! { on ["comment"] prefilter = ["/**"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in scan_blocks(text) {
        for tag in block.tags() {
            if !matches!(tag.name.as_str(), "param" | "arg" | "argument") {
                continue;
            }
            if !has_description(&tag.body) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: tag.line + line_offset,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message:
                        "`@param` is missing a description — explain what this parameter represents, not just its type."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

/// Does the `@param` body contain a description (anything beyond the
/// type + name)?
fn has_description(body: &str) -> bool {
    let after_type = strip_leading_type(body);
    // Drop the param name (first whitespace-delimited token). The
    // remainder, after optional leading `-` / `:`, is the description.
    let mut rest = after_type.splitn(2, char::is_whitespace);
    let Some(_name) = rest.next() else {
        return false;
    };
    let Some(tail) = rest.next() else {
        return false;
    };
    let tail = tail.trim_start_matches(|c: char| c == '-' || c == ':' || c.is_whitespace());
    !tail.trim().is_empty()
}

fn strip_leading_type(body: &str) -> &str {
    let trimmed = body.trim_start();
    if !trimmed.starts_with('{') {
        return trimmed;
    }
    let mut depth = 0usize;
    for (i, ch) in trimmed.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return trimmed[i + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_param_with_no_description() {
        let src = "/**\n * @param {string} x\n */\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_param_with_only_dash() {
        let src = "/**\n * @param {string} x -\n */\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_param_with_description() {
        let src = "/**\n * @param {string} x - the user id\n */\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_param_without_type() {
        let src = "/**\n * @param x - the id\n */\n";
        assert!(run(src).is_empty());
    }
}
