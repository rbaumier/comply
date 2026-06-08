//! jsdoc/valid-types OxcCheck backend — scan comments for invalid type exprs.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use std::sync::Arc;

pub struct Check;

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
    let _ = end_idx;
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            // oxc comment spans include the `//` or `/* */` markers
            let text = &ctx.source[start..end];
            if !text.starts_with("/**") {
                continue;
            }
            let (line_offset, _) = byte_offset_to_line_col(ctx.source, start);

            for block in scan_blocks(text) {
                for tag in block.tags() {
                    if !TYPED_TAGS.contains(&tag.name.as_str()) {
                        continue;
                    }
                    let trimmed = tag.body.trim_start();
                    if !trimmed.starts_with('{') {
                        continue;
                    }
                    if let Some(reason) = validate_type_expr(trimmed) {
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            // tag.line is 1-based relative to the comment block;
                            // line_offset is the 1-based absolute line of the comment start.
                            line: tag.line + line_offset - 1,
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

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
