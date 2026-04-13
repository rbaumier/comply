//! jsdoc-sort-tags backend — JSDoc tags must follow canonical order:
//! @param, @returns, @throws, @example.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

fn tag_order(tag: &str) -> Option<u8> {
    match tag {
        "param" => Some(0),
        "returns" | "return" => Some(1),
        "throws" | "exception" => Some(2),
        "example" => Some(3),
        _ => None,
    }
}

fn extract_tag_name(content: &str) -> Option<&str> {
    let rest = content.strip_prefix('@')?;
    let tag = rest
        .split(|c: char| c.is_whitespace() || c == '{')
        .next()?;
    if tag.is_empty() {
        return None;
    }
    Some(tag)
}

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();

        walk_tree(tree, |node| {
            if node.kind() != "comment" {
                return;
            }
            let Ok(text) = node.utf8_text(source_bytes) else { return };
            if !text.starts_with("/**") {
                return;
            }

            let comment_start_line = node.start_position().row;

            // Collect ordered tags with their positions.
            let mut seen_tags: Vec<(u8, &str, usize)> = Vec::new();
            for (rel_idx, line) in text.lines().enumerate() {
                let content = line.trim().trim_start_matches('*').trim();
                if let Some(tag) = extract_tag_name(content)
                    && let Some(order) = tag_order(tag) {
                        seen_tags.push((order, tag, comment_start_line + rel_idx));
                    }
            }

            for window in seen_tags.windows(2) {
                let (prev_order, prev_tag, _) = window[0];
                let (cur_order, cur_tag, cur_abs_line) = window[1];
                if cur_order < prev_order {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: cur_abs_line + 1,
                        column: 1,
                        rule_id: "jsdoc-sort-tags".into(),
                        message: format!(
                            "`@{cur_tag}` must come before `@{prev_tag}`. \
                             Canonical order: @param, @returns, @throws, @example."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        });

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_returns_before_param() {
        let source = r#"
/**
 * Does something.
 * @returns the result
 * @param x - input
 */
function foo(x: number) { return x; }
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("@param"));
        assert!(d[0].message.contains("before"));
    }

    #[test]
    fn allows_correct_order() {
        let source = r#"
/**
 * Does something.
 * @param x - input
 * @returns the result
 * @throws if invalid
 * @example foo(1)
 */
function foo(x: number) { return x; }
"#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_example_before_throws() {
        let source = r#"
/**
 * Does something.
 * @param x - input
 * @example foo(1)
 * @throws Error
 */
function foo(x: number) { return x; }
"#;
        let d = run_on(source);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("@throws"));
    }
}
