//! sql-boolean-column-prefix — Drizzle ORM backend.
//!
//! Flags `boolean('col')` calls where `col` doesn't start with
//! `is_` or `has_`. The prefix makes boolean semantics obvious at
//! call sites.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["call_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        let Ok(name) = function.utf8_text(source_bytes) else {
            return;
        };
        if name != "boolean" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        for i in 0..args.named_child_count() {
            let Some(arg) = args.named_child(i) else {
                continue;
            };
            if arg.kind() == "string" {
                let Ok(raw) = arg.utf8_text(source_bytes) else {
                    continue;
                };
                let col_name = raw.trim_matches(|c| c == '"' || c == '\'' || c == '`');
                let lower = col_name.to_ascii_lowercase();
                if !lower.starts_with("is_") && !lower.starts_with("has_") {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "BOOLEAN column `{col_name}` should be prefixed with \
                             `is_` or `has_` — the prefix makes boolean semantics \
                             obvious at call sites."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                break;
            }
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
    fn flags_boolean_active() {
        let src = "const active = boolean('active');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_boolean_admin() {
        let src = "const admin = boolean('admin');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_is_prefix() {
        let src = "const isActive = boolean('is_active');";
        assert!(run(src).is_empty());
    }

    #[test]
    fn does_not_flag_has_prefix() {
        let src = "const hasRole = boolean('has_role');";
        assert!(run(src).is_empty());
    }
}
