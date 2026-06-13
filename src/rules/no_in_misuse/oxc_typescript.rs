//! no-in-misuse oxc backend — flag `x in arr` where `arr` looks like an array.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BinaryOperator;
use oxc_span::GetSpan;
use std::sync::Arc;

const ARRAY_HINTS: &[&str] = &[
    "arr", "list", "items", "elements", "values", "entries", "rows", "results",
];

/// Trailing word segments marking a single extracted element rather than the
/// collection itself (`listItem`, `rowEntry`), so the RHS is not an array.
const SINGULAR_SUFFIXES: &[&str] = &["item", "entry", "element", "value", "row", "result"];

/// Split an identifier into lowercase word segments across camelCase, snake_case
/// and kebab boundaries (`listItem` -> `["list", "item"]`).
fn word_segments(name: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut prev_lower = false;
    for ch in name.chars() {
        if ch == '_' || ch == '-' || ch == '$' || ch == '.' {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
            prev_lower = false;
            continue;
        }
        if ch.is_ascii_uppercase() && prev_lower && !current.is_empty() {
            segments.push(std::mem::take(&mut current));
        }
        current.push(ch.to_ascii_lowercase());
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

/// Whether the RHS identifier names a collection. A whole word segment must
/// match an array hint, and the final segment must not be a singular suffix —
/// `listItem` is one element of `list`, not the array.
fn looks_like_array_name(name: &str) -> bool {
    let segments = word_segments(name);
    if let Some(last) = segments.last()
        && SINGULAR_SUFFIXES.contains(&last.as_str())
    {
        return false;
    }
    segments
        .iter()
        .any(|seg| ARRAY_HINTS.contains(&seg.as_str()))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };

        if bin.operator != BinaryOperator::In {
            return;
        }

        // Skip `for ... in` — the parent is a ForInStatement.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(parent.kind(), AstKind::ForInStatement(_)) {
            return;
        }

        let rhs_start = bin.right.span().start as usize;
        let rhs_end = bin.right.span().end as usize;
        let rhs_text = &ctx.source[rhs_start..rhs_end];

        let looks_like_array = rhs_text.starts_with('[') || looks_like_array_name(rhs_text);

        if !looks_like_array {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`in` operator checks object keys, not array values — use `.includes()` instead.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_in_on_plural_array_name() {
        assert_eq!(run_on("if (\"x\" in myItems) {}").len(), 1);
    }

    #[test]
    fn flags_in_on_list_suffix() {
        assert_eq!(run_on("if (val in userList) {}").len(), 1);
    }

    #[test]
    fn flags_in_on_array_literal() {
        assert_eq!(run_on("if (\"x\" in [1, 2, 3]) {}").len(), 1);
    }

    #[test]
    fn allows_for_in_loop() {
        assert!(run_on("for (const key in obj) {}").is_empty());
    }

    #[test]
    fn allows_in_on_object() {
        assert!(run_on("if (\"name\" in config) {}").is_empty());
    }

    // Regression: vercel/commerce — `listItem` is a single discriminated-union
    // element, not an array. The `list` substring must not classify it as one.
    #[test]
    fn allows_in_on_list_item_element() {
        let src = r#"if ("path" in listItem && pathname === listItem.path) {}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_in_on_member_list_item() {
        assert!(run_on(r#"if ("slug" in row.listItem) {}"#).is_empty());
    }

    #[test]
    fn flags_in_on_member_list() {
        assert_eq!(run_on(r#"if ("slug" in this.userList) {}"#).len(), 1);
    }
}
