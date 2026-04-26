//! jsdoc/require-hyphen-before-param-description — if a `@param`
//! has a description, the name must be followed by ` - ` before it.
//!
//! This is a stylistic consistency rule from eslint-plugin-jsdoc.
//! The hyphen makes it visually obvious where the name ends and the
//! prose begins, which matters when the type expression is long.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_helpers::scan_blocks;

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in scan_blocks(text) {
        for tag in block.tags() {
            if !matches!(tag.name.as_str(), "param" | "arg" | "argument") {
                continue;
            }
            if missing_hyphen(&tag.body) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: tag.line + line_offset,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message:
                        "Insert a `-` between the @param name and its description for readability."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

/// Returns true if the param has a description but no ` - ` separator
/// between the name and that description.
fn missing_hyphen(body: &str) -> bool {
    let after_type = strip_leading_type(body);
    // Split off the name.
    let mut it = after_type.splitn(2, char::is_whitespace);
    let Some(_name) = it.next() else {
        return false;
    };
    let Some(tail) = it.next() else {
        return false; // no description at all — different rule
    };
    let tail = tail.trim_start();
    if tail.is_empty() {
        return false;
    }
    !tail.starts_with('-')
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
    fn flags_missing_hyphen() {
        let src = "/**\n * @param {string} id the user id\n */\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_hyphen_separator() {
        let src = "/**\n * @param {string} id - the user id\n */\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_param_without_description() {
        let src = "/**\n * @param {string} id\n */\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_param_tags() {
        let src = "/**\n * @returns {string} the id\n */\n";
        assert!(run(src).is_empty());
    }
}
