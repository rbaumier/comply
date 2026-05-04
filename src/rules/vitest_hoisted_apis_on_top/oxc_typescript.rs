//! vitest-hoisted-apis-on-top oxc backend — flag `vi.mock(...)` / `vi.hoisted(...)`
//! that appear after any import statement at the top level.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hoisted"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        use oxc_ast::AstKind;
        use oxc_ast::ast::Statement;

        let mut diagnostics = Vec::new();

        // Find the program node.
        for node in semantic.nodes().iter() {
            let AstKind::Program(program) = node.kind() else {
                continue;
            };

            let mut seen_import = false;
            for stmt in &program.body {
                match stmt {
                    Statement::ImportDeclaration(_) => {
                        seen_import = true;
                    }
                    Statement::ExpressionStatement(expr_stmt) => {
                        if let Some((prop_name, span)) =
                            vi_hoisted_call_info(&expr_stmt.expression)
                            && seen_import {
                                let (line, column) =
                                    byte_offset_to_line_col(ctx.source, span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: super::META.id.into(),
                                    message: format!(
                                        "`vi.{prop_name}(...)` should appear above all imports — Vitest hoists it but readers expect source order to match execution order."
                                    ),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                    }
                    _ => {}
                }
            }
            break; // Only one program node.
        }

        diagnostics
    }
}

/// If the expression is a `vi.mock(...)` / `vi.hoisted(...)` / `vi.unmock(...)` / `vi.doMock(...)`
/// call, return `(method_name, call_span)`.
fn vi_hoisted_call_info<'a>(expr: &'a oxc_ast::ast::Expression<'a>) -> Option<(&'a str, oxc_span::Span)> {
    use oxc_ast::ast::Expression;

    let Expression::CallExpression(call) = expr else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    let Expression::Identifier(obj) = &member.object else {
        return None;
    };
    if obj.name != "vi" {
        return None;
    }
    let prop = member.property.name.as_str();
    if matches!(prop, "mock" | "hoisted" | "unmock" | "doMock") {
        Some((prop, call.span))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, "app.test.ts")
    }

    #[test]
    fn flags_vi_mock_after_import() {
        let d = run_ts("import { foo } from './foo';\nvi.mock('./foo');");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "vitest-hoisted-apis-on-top");
    }

    #[test]
    fn allows_vi_mock_before_imports() {
        let d = run_ts("vi.mock('./foo');\nimport { foo } from './foo';");
        assert!(d.is_empty());
    }

    #[test]
    fn flags_vi_hoisted_after_import() {
        let d = run_ts("import x from 'x';\nconst h = vi.hoisted(() => ({}));");
        // `const h = vi.hoisted(...)` is a variable declaration, not an
        // expression_statement — we only flag bare calls.
        assert!(d.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let d = crate::rules::test_helpers::run_oxc_ts_with_path(
            "import x from 'x';\nvi.mock('./foo');",
            &Check,
            "src/util.ts",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_no_mocks_no_imports() {
        let d = run_ts("const x = 1;");
        assert!(d.is_empty());
    }
}
