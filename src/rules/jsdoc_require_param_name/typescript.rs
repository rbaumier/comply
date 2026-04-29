//! jsdoc/require-param-name — flag `@param` tags that have only a
//! type expression, no parameter name.
//!
//! `@param {string}` with no name is ambiguous: doc generators cannot
//! line it up against the function signature, so the param silently
//! becomes undocumented.

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
            if !has_name(&tag.body) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: tag.line + line_offset,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message:
                        "`@param` is missing a parameter name — add the name after the optional `{type}`."
                            .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

fn has_name(body: &str) -> bool {
    let after_type = strip_leading_type(body).trim_start();
    let first = match after_type.split_whitespace().next() {
        Some(t) => t,
        None => return false,
    };
    // Strip brackets + default value for optional params: `[name]`,
    // `[name=default]`.
    let cleaned = first.trim_start_matches('[').trim_end_matches(']');
    let name = cleaned.split('=').next().unwrap_or("");
    is_valid_ident(name)
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

fn is_valid_ident(s: &str) -> bool {
    let mut chars = s.chars();
    // Allow dotted / bracketed names (`options.foo`, `options['foo']`)
    // that JSDoc supports for destructured params. The first segment
    // must still be a plain identifier.
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.' | '[' | ']' | '\''))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_param_with_only_type() {
        let src = "/**\n * @param {string}\n */\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_param() {
        let src = "/**\n * @param\n */\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_param_with_name_only() {
        let src = "/**\n * @param x\n */\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_type_plus_name() {
        let src = "/**\n * @param {string} id - user\n */\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_optional_param_name() {
        let src = "/**\n * @param {string} [id] - optional\n */\n";
        assert!(run(src).is_empty());
    }
}
