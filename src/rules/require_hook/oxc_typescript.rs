//! require-hook oxc backend — in test files, flag top-level statements
//! that have side effects (function calls, assignments) which belong
//! inside a `beforeEach` / `beforeAll` hook so tests can control them.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("_test.")
}

/// Framework callees that are allowed to appear at the top level of a
/// test file: test/suite declarations and the hooks themselves.
fn is_allowed_test_callee(name: &str) -> bool {
    matches!(
        name,
        "describe"
            | "fdescribe"
            | "xdescribe"
            | "suite"
            | "context"
            | "test"
            | "it"
            | "fit"
            | "xit"
            | "xtest"
            | "beforeEach"
            | "afterEach"
            | "beforeAll"
            | "afterAll"
            | "before"
            | "after"
            | "contextualize"
    )
}

/// Strip chained member access / `.only` / `.skip` / `.each(...)` suffixes
/// so `describe.skip(...)`, `it.only(...)`, `test.each([...])(...)`
/// resolve to their root identifier.
fn root_callee_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(mem) => root_callee_name(&mem.object),
        Expression::CallExpression(call) => root_callee_name(&call.callee),
        _ => None,
    }
}

/// Calls that *must* live at module scope because the test runner hoists
/// or otherwise binds them to file-level evaluation.
fn is_hoisted_test_api(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::StaticMemberExpression(mem) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &mem.object else {
        return false;
    };
    let obj_name = obj.name.as_str();
    let prop_name = mem.property.name.as_str();
    matches!(
        (obj_name, prop_name),
        ("vi", "mock")
            | ("vi", "unmock")
            | ("vi", "doMock")
            | ("vi", "doUnmock")
            | ("vi", "hoisted")
            | ("vi", "stubEnv")
            | ("vi", "stubGlobal")
            | ("jest", "mock")
            | ("jest", "unmock")
            | ("jest", "doMock")
            | ("jest", "dontMock")
    )
}

fn is_commonjs_import(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    if callee.name.as_str() != "require" {
        return false;
    }
    if call.arguments.len() != 1 {
        return false;
    }
    matches!(&call.arguments[0], Argument::StringLiteral(_))
}

/// Is this initializer pure enough to allow at module scope?
fn is_pure_initializer(expr: &Expression) -> bool {
    if is_hoisted_test_api(expr) {
        return true;
    }
    if is_commonjs_import(expr) {
        return true;
    }
    match expr {
        Expression::StringLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::Identifier(_)
        | Expression::ArrowFunctionExpression(_)
        | Expression::FunctionExpression(_)
        | Expression::ClassExpression(_) => true,
        Expression::TemplateLiteral(t) => t.expressions.is_empty(),
        Expression::ArrayExpression(arr) => arr.elements.iter().all(|el| match el {
            ArrayExpressionElement::SpreadElement(_) => false,
            ArrayExpressionElement::Elision(_) => true,
            _ => {
                if let Some(inner) = el.as_expression() {
                    is_pure_initializer(inner)
                } else {
                    false
                }
            }
        }),
        Expression::ObjectExpression(obj) => obj.properties.iter().all(|prop| match prop {
            ObjectPropertyKind::ObjectProperty(p) => is_pure_initializer(&p.value),
            ObjectPropertyKind::SpreadProperty(_) => false,
        }),
        Expression::UnaryExpression(u) => is_pure_initializer(&u.argument),
        Expression::ParenthesizedExpression(p) => is_pure_initializer(&p.expression),
        Expression::TSAsExpression(e) => is_pure_initializer(&e.expression),
        Expression::TSTypeAssertion(e) => is_pure_initializer(&e.expression),
        Expression::TSSatisfiesExpression(e) => is_pure_initializer(&e.expression),
        Expression::TSNonNullExpression(e) => is_pure_initializer(&e.expression),
        _ => false,
    }
}

/// Every declarator in a declaration must have a pure initializer (or none).
fn declaration_is_pure(decl: &VariableDeclaration) -> bool {
    decl.declarations.iter().all(|d| {
        d.init
            .as_ref()
            .is_none_or(|init| is_pure_initializer(init))
    })
}

fn imports_node_test(program: &Program<'_>) -> bool {
    program.body.iter().any(|stmt| {
        if let Statement::ImportDeclaration(import) = stmt {
            import.source.value.as_str() == "node:test"
        } else {
            false
        }
    })
}

/// Classify a top-level statement.
fn top_level_is_allowed(stmt: &Statement, node_test_mode: bool) -> bool {
    match stmt {
        Statement::ImportDeclaration(_)
        | Statement::ExportAllDeclaration(_)
        | Statement::ExportDefaultDeclaration(_)
        | Statement::ExportNamedDeclaration(_)
        | Statement::FunctionDeclaration(_)
        | Statement::ClassDeclaration(_)
        | Statement::TSInterfaceDeclaration(_)
        | Statement::TSTypeAliasDeclaration(_)
        | Statement::TSEnumDeclaration(_)
        | Statement::TSModuleDeclaration(_)
        | Statement::EmptyStatement(_) => true,

        Statement::VariableDeclaration(decl) => declaration_is_pure(decl),

        Statement::ExpressionStatement(expr_stmt) => {
            let expr = &expr_stmt.expression;
            // String directives like 'use strict'.
            if matches!(expr, Expression::StringLiteral(_)) {
                return true;
            }
            // node:test: top-level await is idiomatic setup — the framework has no beforeAll equivalent
            if node_test_mode && matches!(expr, Expression::AwaitExpression(_)) {
                return true;
            }
            // Must be a call expression.
            if !matches!(expr, Expression::CallExpression(_)) {
                return false;
            }
            if is_hoisted_test_api(expr) {
                return true;
            }
            let Some(name) = root_callee_name(expr) else {
                return false;
            };
            is_allowed_test_callee(name)
        }

        _ => false,
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
    
    #[test]
    fn allows_contextualize_at_top_level() {
        let src = r#"
import { contextualize } from "@ark/attest";
contextualize(() => {
  describe("t", () => { it("works", () => {}); });
});
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert!(d.is_empty(), "contextualize() must be allowed at top level: {d:?}");
    }

    #[test]
    fn allows_top_level_await_with_node_test_import() {
        let src = r#"
import { test } from "node:test";
await setup();
test("x", () => {});
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert!(d.is_empty(), "top-level await must be allowed when node:test is imported: {d:?}");
    }

    #[test]
    fn flags_top_level_await_without_node_test_import() {
        let src = r#"
await setup();
describe("x", () => {});
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(d.len(), 1, "top-level await without node:test must be flagged: {d:?}");
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_test_file(ctx.path) {
            return Vec::new();
        }

        let program = semantic.nodes().program();
        let node_test_mode = imports_node_test(program);
        let mut diagnostics = Vec::new();

        for stmt in &program.body {
            if top_level_is_allowed(stmt, node_test_mode) {
                continue;
            }
            let span = stmt.span();
            let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message:
                    "Top-level side effect in a test file — move it into a `beforeEach` or `beforeAll` hook."
                        .into(),
                severity: Severity::Warning,
                span: Some((span.start as usize, (span.end - span.start) as usize)),
            });
        }

        diagnostics
    }
}
