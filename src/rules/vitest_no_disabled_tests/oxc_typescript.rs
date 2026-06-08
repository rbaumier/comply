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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path;



    fn run_oxc_ts(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(source, &Check, "app.test.ts")
    }


    #[test]
    fn flags_xtest() {
        let d = run_oxc_ts("xtest('broken', () => { expect(1).toBe(1); });");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "vitest-no-disabled-tests");
    }


    #[test]
    fn flags_xit() {
        let d = run_oxc_ts("xit('broken', () => {});");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_xdescribe() {
        let d = run_oxc_ts("xdescribe('suite', () => {});");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_test_skip() {
        let d = run_oxc_ts("test.skip('broken', () => {});");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_regular_test() {
        let d = run_oxc_ts("test('works', () => { expect(1).toBe(1); });");
        assert!(d.is_empty());
    }


    #[test]
    fn ignores_non_test_file() {
        let d = run_oxc_ts_with_path("xtest('a', () => {});", &Check, "src/util.ts");
        assert!(d.is_empty());
    }


    #[test]
    fn flags_skip_with_vitest_import_no_marker() {
        let d = run_oxc_ts_with_path(
            "import { it } from 'vitest';\nit.skip('login', () => {});",
            &Check,
            "tests/login.ts",
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "vitest-no-disabled-tests");
    }


    #[test]
    fn ignores_no_marker_no_import() {
        let d = run_oxc_ts_with_path("it.skip('login', () => {});", &Check, "tests/login.ts");
        assert!(d.is_empty());
    }
}
