//! OXC backend for jsdoc/require-param-name — flag `@param` tags missing a name.
//! Uses semantic comments API instead of per-node dispatch.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
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
            let start = comment.content_span().start as usize;
            let end = comment.content_span().end as usize;
            // Reconstruct full comment text with delimiters for scan_blocks
            let raw = &ctx.source[start..end];
            let full_text = format!("/*{}*/", raw);
            if !full_text.starts_with("/**") {
                continue;
            }

            let (comment_line, _) = byte_offset_to_line_col(ctx.source, start.saturating_sub(2));

            for block in scan_blocks(&full_text) {
                for tag in block.tags() {
                    if !matches!(tag.name.as_str(), "param" | "arg" | "argument") {
                        continue;
                    }
                    if !has_name(&tag.body) {
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line: tag.line + comment_line - 1,
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

        diagnostics
    }
}

fn has_name(body: &str) -> bool {
    let after_type = strip_leading_type(body).trim_start();
    let first = match after_type.split_whitespace().next() {
        Some(t) => t,
        None => return false,
    };
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
    match chars.next() {
        Some(c) if c.is_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.' | '[' | ']' | '\''))
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_param_with_only_type() {
        let src = "/**\n * @param {string}\n */\nfunction f(x) {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_param() {
        let src = "/**\n * @param\n */\nfunction f(x) {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_param_with_name_only() {
        let src = "/**\n * @param x\n */\nfunction f(x) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_type_plus_name() {
        let src = "/**\n * @param {string} id - user\n */\nfunction f(id) {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_optional_param_name() {
        let src = "/**\n * @param {string} [id] - optional\n */\nfunction f(id) {}";
        assert!(run(src).is_empty());
    }
}
