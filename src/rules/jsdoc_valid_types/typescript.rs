//! jsdoc/valid-types — flag syntactically invalid JSDoc type
//! expressions (unbalanced braces/parens, empty `{}`, trailing
//! pipes).
//!
//! A broken type expression is a silent failure: JSDoc tooling either
//! emits `any` or skips the parameter entirely, erasing the very
//! contract the comment was meant to encode.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsdoc_helpers::scan_blocks;

/// Tags that are expected to carry a `{...}` type expression.
const TYPED_TAGS: &[&str] = &[
    "param",
    "arg",
    "argument",
    "returns",
    "return",
    "type",
    "typedef",
    "property",
    "prop",
    "throws",
    "exception",
    "yields",
    "yield",
];

crate::ast_check! { on ["comment"] prefilter = ["/**"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !text.starts_with("/**") { return; }
    let line_offset = node.start_position().row;

    for block in scan_blocks(text) {
        for tag in block.tags() {
            if !TYPED_TAGS.contains(&tag.name.as_str()) {
                continue;
            }
            let trimmed = tag.body.trim_start();
            if !trimmed.starts_with('{') {
                // No type expression — other rules will complain
                // if one was required.
                continue;
            }
            if let Some(reason) = validate_type_expr(trimmed) {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: tag.line + line_offset,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "`@{}` has an invalid type expression: {reason}",
                        tag.name
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

/// Validate the leading `{...}` in `s`. Returns `Some(reason)` when
/// the expression is malformed, `None` when it looks well-formed.
fn validate_type_expr(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut depth: i32 = 0;
    let mut paren: i32 = 0;
    let mut inside_type: Option<String> = None;
    let mut end_idx = 0usize;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    inside_type = Some(s[1..i].to_string());
                    end_idx = i;
                    break;
                }
                if depth < 0 {
                    return Some("mismatched `}`".into());
                }
            }
            b'(' => paren += 1,
            b')' => paren -= 1,
            _ => {}
        }
    }
    if depth != 0 {
        return Some("unbalanced `{` / `}`".into());
    }
    let Some(inner) = inside_type else {
        return Some("missing closing `}`".into());
    };
    if paren != 0 {
        // Count parens in inner only — leading paren imbalance in the
        // tail after `}` is not this tag's problem.
        let inner_paren = inner.chars().filter(|c| *c == '(').count() as i32
            - inner.chars().filter(|c| *c == ')').count() as i32;
        if inner_paren != 0 {
            return Some("unbalanced `(` / `)`".into());
        }
    }
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return Some("empty `{}` type".into());
    }
    if trimmed.ends_with('|') || trimmed.starts_with('|') {
        return Some("dangling `|` in union".into());
    }
    if trimmed.ends_with(',') {
        return Some("trailing `,` in type".into());
    }
    // Block unused to silence clippy in release; end_idx tracked for
    // potential future column reporting.
    let _ = end_idx;
    None
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_empty_type() {
        let src = "/**\n * @param {} x\n */\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("empty"));
    }

    #[test]
    fn flags_unbalanced_braces() {
        let src = "/**\n * @param {string x\n */\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_dangling_pipe() {
        let src = "/**\n * @param {string |} x\n */\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_valid_types() {
        let src = r#"
/**
 * @param {string} x
 * @param {Array<number>} arr
 * @returns {Promise<void>}
 */
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_union_type() {
        let src = "/**\n * @param {string | number} x\n */\n";
        assert!(run(src).is_empty());
    }
}
