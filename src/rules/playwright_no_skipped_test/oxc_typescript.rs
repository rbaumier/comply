//! playwright-no-skipped-test OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

const TEST_FNS: &[&str] = &["test", "it", "describe"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["skip"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }
        if !crate::rules::playwright::is_playwright_context(ctx) {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Match test.skip(...), describe.skip(...), it.skip(...)
        if let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee {
            if member.property.name.as_str() != "skip" {
                // Check chained: test.skip.each(...)
                // member.object would be test.skip
                if let oxc_ast::ast::Expression::StaticMemberExpression(inner) =
                    &member.object
                    && inner.property.name.as_str() == "skip"
                        && let oxc_ast::ast::Expression::Identifier(obj) = &inner.object
                            && TEST_FNS.contains(&obj.name.as_str()) {
                                let (line, column) = byte_offset_to_line_col(
                                    ctx.source,
                                    call.span.start as usize,
                                );
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: super::META.id.into(),
                                    message:
                                        "Unexpected use of the `.skip()` annotation.".into(),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                return;
            }
            if let oxc_ast::ast::Expression::Identifier(obj) = &member.object
                && TEST_FNS.contains(&obj.name.as_str()) {
                    let (line, column) =
                        byte_offset_to_line_col(ctx.source, call.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Unexpected use of the `.skip()` annotation.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
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
    
    const PW_IMPORT: &str = "import { test, expect } from \"@playwright/test\";\n";

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, &format!("{PW_IMPORT}{source}"), "app.test.ts")
    }

    #[test]
    fn flags_test_skip() {
        let d = run_ts("test.skip('broken', () => {});");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "playwright-no-skipped-test");
    }

    #[test]
    fn flags_describe_skip() {
        let d = run_ts("describe.skip('suite', () => {});");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_test_only() {
        let d = run_ts("test.only('focused', () => {});");
        assert!(d.is_empty());
    }
}
