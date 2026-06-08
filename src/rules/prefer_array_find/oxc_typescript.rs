//! prefer-array-find OxcCheck backend — flag `.filter(…)[0]`, `.filter(…).at(0)`,
//! and `.filter(…).shift()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression, AstType::ComputedMemberExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["filter"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            // Pattern 1: `.filter(…)[0]` — ComputedMemberExpression
            AstKind::ComputedMemberExpression(mem) => {
                // The object must be a call to `.filter(...)`
                let Expression::CallExpression(call) = &mem.object else { return };
                let Expression::StaticMemberExpression(callee) = &call.callee else { return };
                if callee.property.name.as_str() != "filter" {
                    return;
                }
                // The index must be `0`
                let Expression::NumericLiteral(num) = &mem.expression else { return };
                if num.value != 0.0 {
                    return;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, mem.span().start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer `.find(…)` over `.filter(…)[0]` — `.find()` short-circuits on the first match.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            // Pattern 2: `.filter(…).at(0)` or `.filter(…).shift()`
            AstKind::CallExpression(call) => {
                let Expression::StaticMemberExpression(callee) = &call.callee else { return };
                let method = callee.property.name.as_str();

                // The object must be a call to `.filter(...)`
                let Expression::CallExpression(inner_call) = &callee.object else { return };
                let Expression::StaticMemberExpression(inner_callee) = &inner_call.callee else {
                    return;
                };
                if inner_callee.property.name.as_str() != "filter" {
                    return;
                }

                match method {
                    "at" => {
                        // Check that the argument is `0`.
                        let Some(first_arg) = call.arguments.first() else { return };
                        let oxc_ast::ast::Argument::NumericLiteral(num) = first_arg else {
                            return;
                        };
                        if num.value != 0.0 {
                            return;
                        }
                    }
                    "shift" => { /* always flag */ }
                    _ => return,
                }

                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Prefer `.find(…)` over `.filter(…)[0]` — `.find()` short-circuits on the first match.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_filter_zero_index() {
        assert_eq!(run_on("const x = arr.filter(fn)[0];").len(), 1);
    }

    #[test]
    fn flags_filter_at_zero() {
        assert_eq!(run_on("const x = arr.filter(fn).at(0);").len(), 1);
    }

    #[test]
    fn flags_filter_shift() {
        assert_eq!(run_on("const x = arr.filter(fn).shift();").len(), 1);
    }

    #[test]
    fn allows_find() {
        assert!(run_on("const x = arr.find(fn);").is_empty());
    }

    #[test]
    fn allows_filter_alone() {
        assert!(run_on("const x = arr.filter(fn);").is_empty());
    }

    #[test]
    fn allows_filter_non_zero_index() {
        assert!(run_on("const x = arr.filter(fn)[1];").is_empty());
    }
}
