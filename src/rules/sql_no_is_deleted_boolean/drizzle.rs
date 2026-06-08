//! sql-no-is-deleted-boolean — Drizzle ORM backend.
//!
//! Flags `boolean('is_deleted')` / `boolean('isDeleted')` calls.
//! Soft-delete markers should carry *when* it happened — use
//! `timestamp('deleted_at', { withTimezone: true })` instead.

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
                if col_name.to_ascii_lowercase().contains("deleted") {
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: super::META.id.into(),
                        message: "Use `timestamp('deleted_at', { withTimezone: true })` \
                                  instead of `boolean('is_deleted')` — a nullable \
                                  timestamp encodes both the boolean and the event time."
                            .into(),
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
    fn flags_boolean_is_deleted_snake() {
        let src = "const isDeleted = boolean('is_deleted');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_boolean_is_deleted_camel() {
        let src = "const deleted = boolean('isDeleted');";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn does_not_flag_other_boolean() {
        let src = "const active = boolean('is_active');";
        assert!(run(src).is_empty());
    }
}
