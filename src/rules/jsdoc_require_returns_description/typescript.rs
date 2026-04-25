//! jsdoc/require-returns-description — flag `@returns` tags that
//! only declare a type (no prose).
//!
//! Same rationale as `require-param-description`: the type is already
//! on the function signature, and "returns a string" is not useful —
//! readers need to know what the string represents.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_helpers::scan_blocks;

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "comment" { return; }
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in scan_blocks(text) {
        for tag in block.tags() {
            if !matches!(tag.name.as_str(), "returns" | "return") {
                continue;
            }
            if !has_description(&tag.body) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: tag.line + line_offset,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message:
                        "`@returns` is missing a description — explain what the return value represents."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

fn has_description(body: &str) -> bool {
    let after_type = strip_leading_type(body);
    let tail = after_type.trim_start_matches(|c: char| c == '-' || c == ':' || c.is_whitespace());
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
    fn flags_returns_with_only_type() {
        let src = "/**\n * @returns {string}\n */\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_returns() {
        let src = "/**\n * @returns\n */\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_returns_with_description() {
        let src = "/**\n * @returns {string} the normalized name\n */\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_returns_without_type() {
        let src = "/**\n * @returns the normalized name\n */\n";
        assert!(run(src).is_empty());
    }
}
