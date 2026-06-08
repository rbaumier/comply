use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "boolean" {
            return;
        }
        for arg in &call.arguments {
            if let Argument::StringLiteral(lit) = arg {
                let col_name = lit.value.as_str();
                if col_name.to_ascii_lowercase().contains("deleted") {
                    let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_is_deleted_snake() {
        assert_eq!(run_on("const isDeleted = boolean('is_deleted');").len(), 1);
    }

    #[test]
    fn flags_is_deleted_camel() {
        assert_eq!(run_on("const deleted = boolean('isDeleted');").len(), 1);
    }

    #[test]
    fn does_not_flag_other_boolean() {
        assert!(run_on("const active = boolean('is_active');").is_empty());
    }
}
