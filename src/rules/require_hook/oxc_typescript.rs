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
        // A binary expression (comparison, `in`, `instanceof`, arithmetic) is a
        // pure read when both operands are pure — e.g. `process.platform === 'win32'`
        // or `'rolldownVersion' in vite`. An impure operand (a call) still fires.
        Expression::BinaryExpression(b) => {
            is_pure_initializer(&b.left) && is_pure_initializer(&b.right)
        }
        // Property access (`process.platform`) is a pure read when its object is pure.
        Expression::StaticMemberExpression(m) => is_pure_initializer(&m.object),
        Expression::ParenthesizedExpression(p) => is_pure_initializer(&p.expression),
        Expression::TSAsExpression(e) => is_pure_initializer(&e.expression),
        Expression::TSTypeAssertion(e) => is_pure_initializer(&e.expression),
        Expression::TSSatisfiesExpression(e) => is_pure_initializer(&e.expression),
        Expression::TSNonNullExpression(e) => is_pure_initializer(&e.expression),
        _ => false,
    }
}

/// Initializer that builds a shared, read-only test fixture by calling a
/// domain function (`buildSchema(...)`, `getTracingChannel(...)`) or a
/// constructor (`new TypeInfo(...)`). Unwraps the same value-preserving
/// wrappers as `is_pure_initializer` so `buildSchema(...) as Schema` counts.
fn is_fixture_builder(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(_) | Expression::NewExpression(_) => true,
        Expression::ParenthesizedExpression(p) => is_fixture_builder(&p.expression),
        Expression::TSAsExpression(e) => is_fixture_builder(&e.expression),
        Expression::TSTypeAssertion(e) => is_fixture_builder(&e.expression),
        Expression::TSSatisfiesExpression(e) => is_fixture_builder(&e.expression),
        Expression::TSNonNullExpression(e) => is_fixture_builder(&e.expression),
        _ => false,
    }
}

/// Every declarator in a declaration must have a pure initializer (or none).
///
/// In `node:test` files a module-scope `const` may also be initialized by a
/// fixture-builder call. `node:test` has no `beforeAll` equivalent at module
/// scope, so building a shared read-only fixture (a parsed schema, a tracing
/// channel) once at import time is the idiomatic pattern there.
fn declaration_is_pure(decl: &VariableDeclaration, node_test_mode: bool) -> bool {
    let allow_fixture_builder = node_test_mode && decl.kind == VariableDeclarationKind::Const;
    decl.declarations.iter().all(|d| {
        d.init.as_ref().is_none_or(|init| {
            is_pure_initializer(init) || (allow_fixture_builder && is_fixture_builder(init))
        })
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

        Statement::VariableDeclaration(decl) => declaration_is_pure(decl, node_test_mode),

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

    #[test]
    fn allows_const_fixture_builder_call_in_node_test_mode() {
        let src = r#"
import { describe, it } from "node:test";
import { buildSchema } from "../buildASTSchema.ts";

const testSchema = buildSchema(`
  interface Pet { name: String }
  type Dog implements Pet { name: String }
`);

describe("TypeInfo", () => {
  it("queries type info", () => {});
});
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "TypeInfo-test.ts");
        assert!(
            d.is_empty(),
            "module-scope const built by a fixture-builder call must be allowed in node:test mode: {d:?}"
        );
    }

    #[test]
    fn allows_const_tracing_channel_in_node_test_mode() {
        let src = r#"
import { describe, it } from "node:test";

const schema = buildSchema(`type Query { field: String }`);
const validateChannel = getTracingChannel('graphql:validate');

describe("diagnostics", () => {
  it("works", () => {});
});
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "diagnostics-test.ts");
        assert!(
            d.is_empty(),
            "module-scope const tracing-channel fixtures must be allowed in node:test mode: {d:?}"
        );
    }

    #[test]
    fn flags_const_fixture_builder_call_without_node_test_import() {
        let src = r#"
import { buildSchema } from "../buildASTSchema";

const schema = buildSchema(`type Query { field: String }`);

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "const call initializer outside node:test mode must still be flagged (jest/vitest have beforeAll): {d:?}"
        );
    }

    #[test]
    fn flags_let_fixture_builder_call_in_node_test_mode() {
        let src = r#"
import { describe, it } from "node:test";

let counter = startCounter();

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "a mutable `let` call initializer must still be flagged even in node:test mode: {d:?}"
        );
    }

    #[test]
    fn flags_bare_setup_call_in_node_test_mode() {
        let src = r#"
import { describe, it } from "node:test";

setup();

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "a bare side-effect call statement must still be flagged in node:test mode: {d:?}"
        );
    }

    #[test]
    fn allows_const_comparison_initializer() {
        let src = r#"
const isWindows = process.platform === 'win32'

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert!(
            d.is_empty(),
            "module-scope const with a pure comparison initializer must be allowed: {d:?}"
        );
    }

    #[test]
    fn allows_const_in_operator_initializer() {
        let src = r#"
const isRolldownVite = 'rolldownVersion' in vite

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert!(
            d.is_empty(),
            "module-scope const with a pure `in` initializer must be allowed: {d:?}"
        );
    }

    #[test]
    fn flags_const_binary_with_call_operand() {
        let src = r#"
const x = sideEffect() === 1

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "a binary expression with a call operand must still be flagged: {d:?}"
        );
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
