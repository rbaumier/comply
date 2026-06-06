//! vitest-no-disabled-tests OXC backend — flag disabled test calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];
const VITEST_IMPORTS: &[&str] = &["from 'vitest'", "from \"vitest\""];
const DISABLED_IDENTIFIERS: &[&str] = &["xtest", "xit", "xdescribe"];
const TEST_FNS: &[&str] = &["test", "it", "describe"];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

fn has_vitest_import(source: &str) -> bool {
    VITEST_IMPORTS.iter().any(|p| crate::oxc_helpers::source_contains(source, p))
}

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
        if !is_test_file(ctx.path) && !has_vitest_import(ctx.source) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        match &call.callee {
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                if DISABLED_IDENTIFIERS.contains(&name) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{name}` disables the test — re-enable or remove it."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            Expression::StaticMemberExpression(member) => {
                let prop = member.property.name.as_str();
                if prop != "skip" {
                    return;
                }
                let Expression::Identifier(obj) = &member.object else {
                    return;
                };
                let obj_name = obj.name.as_str();
                if TEST_FNS.contains(&obj_name) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "`{obj_name}.skip(...)` disables the test — re-enable or remove it."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
            _ => {}
        }
    }
}
