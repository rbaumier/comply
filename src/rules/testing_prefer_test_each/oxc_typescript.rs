use std::sync::Arc;

use oxc_ast::AstKind;
use oxc_ast::ast::{Expression, ModuleExportName, Statement};

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// Returns true if the identifier resolves to a binding imported from `"vitest"` or `"vitest/*"`.
fn ident_is_from_vitest<'a>(
    ident: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::ImportDeclaration(import) = kind {
            let src = import.source.value.as_str();
            return src == "vitest" || src.starts_with("vitest/");
        }
    }
    false
}

/// Returns `true` if the identifier's **imported** name (not the local alias) is `"test"` or `"it"`.
///
/// Handles `import { test as myTest } from "vitest"` — resolves to the `ImportSpecifier`
/// and checks `spec.imported`, so the local alias is irrelevant.
fn imported_name_is_test_or_it<'a>(
    ident: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    match nodes.kind(decl_node_id) {
        AstKind::ImportSpecifier(spec) => match &spec.imported {
            ModuleExportName::IdentifierName(name) => {
                matches!(name.name.as_str(), "test" | "it")
            }
            ModuleExportName::StringLiteral(s) => {
                matches!(s.value.as_str(), "test" | "it")
            }
            _ => false,
        },
        _ => false,
    }
}

/// Returns `true` if `expr` is `test(...)` / `it(...)` / `test.only(...)`
/// / `it.skip(...)` etc. AND the receiver `test` / `it` identifier
/// resolves to a binding imported from `vitest`.
///
/// Checks the **imported** name (not the local alias) to handle
/// `import { test as myTest } from "vitest"` correctly.
fn is_vitest_test_call<'a>(expr: &Expression<'a>, semantic: &'a oxc_semantic::Semantic<'a>) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    match &call.callee {
        Expression::Identifier(ident) => {
            imported_name_is_test_or_it(ident, semantic) && ident_is_from_vitest(ident, semantic)
        }
        Expression::StaticMemberExpression(member) => {
            let Expression::Identifier(obj) = &member.object else {
                return false;
            };
            imported_name_is_test_or_it(obj, semantic) && ident_is_from_vitest(obj, semantic)
        }
        _ => false,
    }
}

/// Recursively check if any statement contains a vitest test call.
fn stmts_contain_test_call<'a>(
    stmts: &[Statement<'a>],
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for stmt in stmts {
        match stmt {
            Statement::ExpressionStatement(es) => {
                if is_vitest_test_call(&es.expression, semantic) {
                    return true;
                }
            }
            Statement::BlockStatement(block) => {
                if stmts_contain_test_call(&block.body, semantic) {
                    return true;
                }
            }
            Statement::IfStatement(if_stmt) => {
                if stmt_contains_test_call(&if_stmt.consequent, semantic) {
                    return true;
                }
                if let Some(alt) = &if_stmt.alternate
                    && stmt_contains_test_call(alt, semantic)
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn stmt_contains_test_call<'a>(
    stmt: &Statement<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    stmts_contain_test_call(std::slice::from_ref(stmt), semantic)
}

fn body_contains_test_call<'a>(
    body: &Statement<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    match body {
        Statement::BlockStatement(block) => stmts_contain_test_call(&block.body, semantic),
        other => stmt_contains_test_call(other, semantic),
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::ForStatement,
            AstType::ForInStatement,
            AstType::ForOfStatement,
            AstType::WhileStatement,
            AstType::CallExpression,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }

        match node.kind() {
            AstKind::ForStatement(stmt) => {
                if body_contains_test_call(&stmt.body, semantic) {
                    push(diagnostics, ctx, stmt.span.start, "for");
                }
            }
            AstKind::ForInStatement(stmt) => {
                if body_contains_test_call(&stmt.body, semantic) {
                    push(diagnostics, ctx, stmt.span.start, "for_in");
                }
            }
            AstKind::ForOfStatement(stmt) => {
                if body_contains_test_call(&stmt.body, semantic) {
                    push(diagnostics, ctx, stmt.span.start, "for_of");
                }
            }
            AstKind::WhileStatement(stmt) => {
                if body_contains_test_call(&stmt.body, semantic) {
                    push(diagnostics, ctx, stmt.span.start, "while");
                }
            }
            AstKind::CallExpression(call) => {
                // Match `xs.forEach(cb)` where `cb` body has a test call.
                let Expression::StaticMemberExpression(member) = &call.callee else {
                    return;
                };
                if member.property.name.as_str() != "forEach" {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else {
                    return;
                };
                let Some(expr) = first_arg.as_expression() else {
                    return;
                };
                match expr {
                    Expression::ArrowFunctionExpression(arrow) => {
                        if stmts_contain_test_call(&arrow.body.statements, semantic) {
                            push(diagnostics, ctx, call.span.start, "forEach");
                        }
                    }
                    Expression::FunctionExpression(func) => {
                        if let Some(body) = &func.body
                            && stmts_contain_test_call(&body.statements, semantic)
                        {
                            push(diagnostics, ctx, call.span.start, "forEach");
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn push(diagnostics: &mut Vec<Diagnostic>, ctx: &CheckCtx, span_start: u32, kind: &str) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!(
            "`{kind}` wraps a `test` / `it` call — replace the loop with `test.each(cases)(...)` so each row is a separate named case."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_oxc_ts_with_path;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_oxc_ts_with_path(source, &Check, "f.test.ts")
    }

    #[test]
    fn flags_for_of_with_vitest_test_call() {
        let src = r#"
import { test } from "vitest";
for (const c of cases) {
  test(c.name, () => { expect(c.input).toBe(c.expected) });
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_for_of_with_vitest_it_call() {
        let src = r#"
import { it } from "vitest";
for (const c of cases) {
  it(c.name, () => {});
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_custom_test_wrapper_from_local_module() {
        let src = r#"
import { test } from "./tx-test";
for (const c of cases) {
  test(c.name, () => { expect(c.input).toBe(c.expected) });
}
"#;
        assert!(
            run(src).is_empty(),
            "custom test wrapper without .each must not be flagged"
        );
    }

    #[test]
    fn ignores_custom_aliased_test_wrapper() {
        let src = r#"
import { txTest as test } from "./tx-test";
for (const c of cases) {
  test(c.name, () => {});
}
"#;
        assert!(
            run(src).is_empty(),
            "aliased custom test wrapper must not be flagged"
        );
    }

    #[test]
    fn ignores_unresolved_test_identifier() {
        // No import for `test` at all — without a binding to vitest, skip the suggestion.
        let src = r#"
for (const c of cases) {
  test(c.name, () => {});
}
"#;
        assert!(
            run(src).is_empty(),
            "unresolved test identifier must not be flagged"
        );
    }

    #[test]
    fn flags_for_of_with_vitest_it_concurrent() {
        let src = r#"
import { it } from "vitest";
for (const { label, build } of cases) {
  it.concurrent(label, async (ctx) => { await build(ctx) });
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_custom_describe_wrapper_from_local_module() {
        let src = r#"
import { myDescribe } from "./helpers";
for (const c of cases) {
  myDescribe(c.label, () => {});
}
"#;
        assert!(
            run(src).is_empty(),
            "custom describe wrapper from local module must not be flagged"
        );
    }

    // Regression tests for aliased vitest import (Fix 1)

    #[test]
    fn flags_aliased_test_import_from_vitest() {
        // `test` aliased to `myTest` — imported name is still `test`, must flag.
        let src = r#"
import { test as myTest } from "vitest";
for (const c of cases) {
  myTest(c.name, () => {});
}
"#;
        assert_eq!(run(src).len(), 1, "aliased vitest `test` must be flagged");
    }

    #[test]
    fn flags_aliased_it_import_from_vitest() {
        // `it` aliased to `myIt` — imported name is still `it`, must flag.
        let src = r#"
import { it as myIt } from "vitest";
for (const c of cases) {
  myIt(c.name, () => {});
}
"#;
        assert_eq!(run(src).len(), 1, "aliased vitest `it` must be flagged");
    }

    #[test]
    fn flags_aliased_test_static_member_from_vitest() {
        // `test as myTest` used as `myTest.skip(...)` — StaticMemberExpression arm.
        let src = r#"
import { test as myTest } from "vitest";
for (const c of cases) {
  myTest.skip(c.name, () => {});
}
"#;
        assert_eq!(
            run(src).len(),
            1,
            "aliased vitest `test` via .skip must be flagged"
        );
    }

    #[test]
    fn ignores_aliased_non_test_import_from_vitest() {
        // `foo` from vitest aliased to `myTest` — imported name is `foo`, not `test`/`it`.
        let src = r#"
import { foo as myTest } from "vitest";
for (const c of cases) {
  myTest(c.name, () => {});
}
"#;
        assert!(
            run(src).is_empty(),
            "vitest import aliased from non-test name must not be flagged"
        );
    }

    // Regression test for for_of label (Fix 2)

    #[test]
    fn for_of_diagnostic_uses_for_of_kind() {
        let src = r#"
import { test } from "vitest";
for (const c of cases) {
  test(c.name, () => {});
}
"#;
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0].message.contains("`for_of`"),
            "ForOfStatement must emit 'for_of' kind, got: {}",
            diags[0].message
        );
    }

    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;
}
