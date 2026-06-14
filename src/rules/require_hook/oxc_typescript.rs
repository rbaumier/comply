//! require-hook oxc backend — in test files, flag top-level statements
//! that have side effects (function calls, assignments) which belong
//! inside a `beforeEach` / `beforeAll` hook so tests can control them.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use oxc_span::GetSpan;
use rustc_hash::FxHashSet;
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
            | "bench"
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

/// Is `expr` a call to an allowed test/suite/hook API?
///
/// Accepts the call when either the root identifier is an allowed callee
/// (`test(...)`, `describe.skip(...)`, `test.each([...])(...)`) or, for a
/// member-expression callee, the property is an allowed callee — the
/// namespaced test runners expose registration through a property
/// (`Deno.test(...)`, `Deno.bench(...)`).
fn callee_is_allowed_test_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    if let Some(name) = root_callee_name(expr)
        && is_allowed_test_callee(name)
    {
        return true;
    }
    if let Expression::StaticMemberExpression(mem) = &call.callee {
        return is_allowed_test_callee(mem.property.name.as_str());
    }
    false
}

/// Is this argument a function body — an arrow or function expression — i.e.
/// the suite callback that registers the nested tests?
fn is_callback_arg(arg: &Argument) -> bool {
    matches!(
        arg.as_expression(),
        Some(Expression::ArrowFunctionExpression(_) | Expression::FunctionExpression(_))
    )
}

/// Is `expr` a curried test-suite-factory invocation of the shape
/// `factory(options)('suite name', () => { ... })`?
///
/// The callee is itself a call (the double-invocation), and the outer call
/// carries a string-literal title plus a function callback — the exact
/// `describe(name, fn)` registration signature. A factory that ultimately
/// expands to `test.describe(...)` registers a suite declaratively, so it
/// belongs at module scope and cannot be moved into a hook. Requiring both
/// the title and the callback keeps genuine side-effectful double-calls
/// (`makeCounter()(5)`, `getInit()()`) flagged.
fn is_suite_factory_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    if !matches!(&call.callee, Expression::CallExpression(_)) {
        return false;
    }
    let has_title = call
        .arguments
        .first()
        .and_then(Argument::as_expression)
        .is_some_and(|e| matches!(e, Expression::StringLiteral(_)));
    has_title && call.arguments.iter().any(is_callback_arg)
}

/// Is `spec` a bare package specifier (`babel-tester`, `@scope/pkg`) rather than a
/// relative or absolute path import? Fixture-runner helpers ship as packages, so a
/// relative `./helpers` binding is never treated as one.
fn is_bare_specifier(spec: &str) -> bool {
    !(spec.starts_with("./") || spec.starts_with("../") || spec.starts_with('/'))
}

/// Local identifier names bound to a default or named import from a bare package
/// specifier. Used to recognise fixture-runner helpers (`import babelTester from
/// 'babel-tester'`, `import { pluginTester } from 'babel-plugin-tester'`) imported
/// from a published test-utility package.
fn package_import_bindings<'a>(program: &Program<'a>) -> FxHashSet<&'a str> {
    let mut out = FxHashSet::default();
    for stmt in &program.body {
        let Statement::ImportDeclaration(import) = stmt else { continue };
        if !is_bare_specifier(import.source.value.as_str()) {
            continue;
        }
        let Some(specifiers) = &import.specifiers else { continue };
        for spec in specifiers {
            match spec {
                ImportDeclarationSpecifier::ImportDefaultSpecifier(def) => {
                    out.insert(def.local.name.as_str());
                }
                ImportDeclarationSpecifier::ImportSpecifier(named) => {
                    out.insert(named.local.name.as_str());
                }
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => {}
            }
        }
    }
    out
}

/// Is `expr` a top-level call to a fixture-runner helper imported from a package —
/// `babelTester('@emotion/babel-plugin', __dirname, { ... })`,
/// `pluginTester({ ... })`?
///
/// These helpers discover fixture files in a directory and register one test case
/// per file: they ARE the test-suite declaration (structurally `describe(...)`),
/// so they belong at module scope and cannot move into a hook. The callee must be a
/// bare identifier following the documented `*Tester` naming convention AND be bound
/// to a package import — a relative `setup()` import or any non-`Tester` call stays
/// flagged.
fn is_fixture_runner_call(expr: &Expression, package_bindings: &FxHashSet<&str>) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    let name = callee.name.as_str();
    name.ends_with("Tester") && package_bindings.contains(name)
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
            | ("vi", "defineHelper")
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

/// A `path.join(...)` / `path.resolve(...)` call whose every argument is a pure
/// initializer. These resolve module-relative paths with no side effects.
fn is_pure_path_call(call: &CallExpression) -> bool {
    let Expression::StaticMemberExpression(mem) = &call.callee else {
        return false;
    };
    let Expression::Identifier(obj) = &mem.object else {
        return false;
    };
    if obj.name.as_str() != "path" {
        return false;
    }
    if !matches!(mem.property.name.as_str(), "join" | "resolve") {
        return false;
    }
    call.arguments.iter().all(|arg| match arg.as_expression() {
        Some(inner) => is_pure_initializer(inner),
        None => false,
    })
}

/// Is `expr` the `import.meta.url` member read?
fn is_import_meta_url(expr: &Expression) -> bool {
    let Expression::StaticMemberExpression(mem) = expr else {
        return false;
    };
    mem.property.name.as_str() == "url" && matches!(&mem.object, Expression::MetaProperty(_))
}

/// A `new URL(stringLiteral, import.meta.url)` call: the standard ESM idiom for
/// resolving a module-relative path. Both arguments are constants — a pure string
/// initializer and the module's own immutable URL — so it computes the same value
/// every load with no observable side effect.
fn is_url_resolution(expr: &Expression) -> bool {
    let Expression::NewExpression(new_expr) = expr else {
        return false;
    };
    let Expression::Identifier(callee) = &new_expr.callee else {
        return false;
    };
    if callee.name.as_str() != "URL" {
        return false;
    }
    if new_expr.arguments.len() != 2 {
        return false;
    }
    let Some(first) = new_expr.arguments[0].as_expression() else {
        return false;
    };
    let Some(second) = new_expr.arguments[1].as_expression() else {
        return false;
    };
    is_pure_initializer(first) && is_import_meta_url(second)
}

/// Is this initializer pure enough to allow at module scope?
fn is_pure_initializer(expr: &Expression) -> bool {
    if is_hoisted_test_api(expr) {
        return true;
    }
    if is_commonjs_import(expr) {
        return true;
    }
    if is_url_resolution(expr) {
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
        // `import.meta` is an immutable, side-effect-free read; this makes
        // `import.meta.dirname` / `import.meta.url` pure via the member arm below.
        Expression::MetaProperty(_) => true,
        // `path.join(...)` / `path.resolve(...)` over pure arguments is a
        // deterministic, side-effect-free module-relative path computation.
        Expression::CallExpression(call) => is_pure_path_call(call),
        // A template literal is a pure read when every interpolation is a plain
        // identifier reference to an already-bound value (e.g.
        // `http://localhost:${PORT}/`). Calls or member access can throw or have
        // side effects, so those still fire. `all` on no interpolations is true.
        Expression::TemplateLiteral(t) => t
            .expressions
            .iter()
            .all(|e| matches!(e, Expression::Identifier(_))),
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
            ObjectPropertyKind::SpreadProperty(s) => is_pure_initializer(&s.argument),
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
fn top_level_is_allowed(
    stmt: &Statement,
    node_test_mode: bool,
    package_bindings: &FxHashSet<&str>,
) -> bool {
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
        | Statement::TSGlobalDeclaration(_)
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
            if is_suite_factory_call(expr) {
                return true;
            }
            if is_fixture_runner_call(expr, package_bindings) {
                return true;
            }
            callee_is_allowed_test_call(expr)
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

    #[test]
    fn allows_const_object_spread_of_pure_source() {
        let src = r#"
const modifiedDefaultConfig: Config = {
  ...defaultConfig,
  changelog: ["@changesets/cli/changelog", null],
};

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert!(
            d.is_empty(),
            "module-scope const built by spreading a pure source must be allowed: {d:?}"
        );
    }

    #[test]
    fn allows_vi_define_helper_at_top_level() {
        let src = r#"
import { test, vi } from 'vitest'
import { page } from 'vitest/browser'

const renderContext = vi.defineHelper(async (context) => {
  document.body.innerHTML = content
  await page.getByRole('list').mark('renderHelper')
})

test('repeated test', { repeats: 2 }, async ({ task }) => {
  await renderContext(task.context)
})
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "retry.test.ts");
        assert!(
            d.is_empty(),
            "module-scope vi.defineHelper() declaration must be allowed: {d:?}"
        );
    }

    #[test]
    fn flags_const_arbitrary_method_call_at_top_level() {
        let src = r#"
import { test, vi } from 'vitest'

const data = service.fetchSync()

test('x', () => {});
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "a non-hoisted member-call initializer must still be flagged: {d:?}"
        );
    }

    #[test]
    fn flags_const_object_spread_of_call() {
        let src = r#"
const x = { ...sideEffect() };

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "an object spread of a call expression must still be flagged: {d:?}"
        );
    }

    #[test]
    fn allows_const_template_literal_with_identifier_interpolation() {
        let src = r#"
import { REGISTRY_MOCK_PORT } from '@pnpm/testing.registry-mock'

const REGISTRY = `http://localhost:${REGISTRY_MOCK_PORT}/`

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "cacheDelete.cmd.test.ts");
        assert!(
            d.is_empty(),
            "module-scope const built from a template literal interpolating an identifier must be allowed: {d:?}"
        );
    }

    #[test]
    fn allows_const_template_literal_with_multiple_identifier_interpolations() {
        let src = r#"
const url = `${HOST}-${PORT}`

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert!(
            d.is_empty(),
            "a template literal interpolating only identifiers must be allowed: {d:?}"
        );
    }

    #[test]
    fn flags_const_template_literal_with_call_interpolation() {
        let src = r#"
const url = `http://localhost:${getPort()}/`

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "a template literal interpolating a call expression must still be flagged: {d:?}"
        );
    }

    #[test]
    fn flags_const_template_literal_with_member_interpolation() {
        let src = r#"
const url = `http://localhost:${obj.port}/`

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "a template literal interpolating a member access must still be flagged: {d:?}"
        );
    }

    #[test]
    fn allows_const_path_join_import_meta_dirname() {
        let src = r#"
import path from 'node:path'

const pnpmBin = path.join(import.meta.dirname, '../../../pnpm/bin/pnpm.mjs')

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "cacheDelete.cmd.test.ts");
        assert!(
            d.is_empty(),
            "module-scope const built by path.join(import.meta.dirname, literal) must be allowed: {d:?}"
        );
    }

    #[test]
    fn allows_const_path_resolve_dirname() {
        let src = r#"
import path from 'node:path'

const fixture = path.resolve(__dirname, './bar')

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert!(
            d.is_empty(),
            "module-scope const built by path.resolve(__dirname, literal) must be allowed: {d:?}"
        );
    }

    #[test]
    fn allows_const_path_join_with_identifier_args() {
        let src = r#"
import path from 'node:path'
import { a, b } from './fixtures'

const fixture = path.join(import.meta.dirname, a, b)

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert!(
            d.is_empty(),
            "path.join with pure identifier args must be allowed: {d:?}"
        );
    }

    #[test]
    fn flags_const_arbitrary_call_initializer() {
        let src = r#"
import { readFileSync } from 'node:fs'

const data = readFileSync('p')

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "an arbitrary call initializer must still be flagged: {d:?}"
        );
    }

    #[test]
    fn flags_const_path_join_with_impure_arg() {
        let src = r#"
import path from 'node:path'

const fixture = path.join(compute(), 'x')

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "path.join with an impure (call) argument must still be flagged: {d:?}"
        );
    }

    #[test]
    fn allows_const_new_url_import_meta_url() {
        let src = r#"
import { test, expect } from '../../playwright.extend'

const DELAY_EXAMPLE = new URL('./delay.mocks.ts', import.meta.url)

test('uses explicit server response delay', async ({ loadExample }) => {
  await loadExample(DELAY_EXAMPLE)
})
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "delay.test.ts");
        assert!(
            d.is_empty(),
            "module-scope const built by new URL(literal, import.meta.url) must be allowed: {d:?}"
        );
    }

    #[test]
    fn allows_const_array_with_new_url_import_meta_url() {
        let src = r#"
const exampleOptions = [
  new URL('./start.mocks.ts', import.meta.url),
  { skipActivation: true },
]

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "start.test.ts");
        assert!(
            d.is_empty(),
            "an array element built by new URL(literal, import.meta.url) must be allowed: {d:?}"
        );
    }

    #[test]
    fn flags_const_new_url_with_dynamic_base() {
        let src = r#"
const target = new URL('./file.ts', getBase())

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "new URL with a non-import.meta.url base must still be flagged: {d:?}"
        );
    }

    #[test]
    fn allows_const_template_literal_without_interpolation() {
        let src = r#"
const url = `http://localhost/`

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert!(
            d.is_empty(),
            "a template literal with no interpolation must remain allowed: {d:?}"
        );
    }

    #[test]
    fn allows_deno_test_at_top_level() {
        let src = r#"
import { assertEquals } from '@std/assert';
import axios from 'axios';

Deno.test('errors: rejects with AxiosError for 500', async () => {
  assertEquals(1, 1);
});

Deno.test('errors: rejects with AxiosError for 404', async () => {
  assertEquals(2, 2);
});
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "error.smoke.test.ts");
        assert!(
            d.is_empty(),
            "Deno.test(...) at top level must be allowed like a bare test(...): {d:?}"
        );
    }

    #[test]
    fn allows_deno_bench_at_top_level() {
        let src = r#"
Deno.bench('perf', () => {});

Deno.test('x', () => {});
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "perf.bench.test.ts");
        assert!(
            d.is_empty(),
            "Deno.bench(...) at top level must be allowed: {d:?}"
        );
    }

    #[test]
    fn flags_member_call_with_non_test_property_at_top_level() {
        let src = r#"
foo.bar();

Deno.test('x', () => {});
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "a member call whose property is not a test callee (foo.bar()) must still be flagged: {d:?}"
        );
    }

    #[test]
    fn allows_curried_suite_factory_call_at_top_level() {
        let src = r#"
import { expect, test } from '@playwright/test';
import { appConfigs } from '../presets';
import { createTestUtils, testAgainstRunningApps } from '../testUtils';

testAgainstRunningApps({ withEnv: [appConfigs.envs.withEmailCodes] })('sign out smoke test @generic', ({ app }) => {
  test.describe.configure({ mode: 'serial' });

  let fakeUser;

  test.beforeAll(async () => {
    const u = createTestUtils({ app });
    fakeUser = u.services.users.createFakeUser();
    await u.services.users.createBapiUser(fakeUser);
  });
});
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "sign-out-smoke.test.ts");
        assert!(
            d.is_empty(),
            "a curried suite-factory call factory(opts)('title', cb) must be allowed at top level: {d:?}"
        );
    }

    #[test]
    fn flags_side_effectful_double_call_without_suite_shape() {
        let src = r#"
makeCounter()(5);

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "a double-call lacking a string title + callback must still be flagged: {d:?}"
        );
    }

    #[test]
    fn flags_double_call_with_title_but_no_callback() {
        let src = r#"
register('feature')('flag');

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "a double-call with a string title but no callback must still be flagged: {d:?}"
        );
    }

    #[test]
    fn allows_declare_global_interface_augmentation() {
        let src = r#"
import { test, expect } from "../../../../playwright.extend"

declare global {
  interface Window {
    request(): Promise<void>
  }
}

test("works", () => { expect(1).toBe(1); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert!(
            d.is_empty(),
            "declare global type augmentation must be allowed at top level: {d:?}"
        );
    }

    #[test]
    fn allows_babel_tester_fixture_runner_at_top_level() {
        let src = r#"
import babelTester from 'babel-tester'
import plugin from '@emotion/babel-plugin'

babelTester('@emotion/babel-plugin', __dirname, {
  plugins: [plugin, [plugin, undefined, 'emotion-copy']]
})
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "index.js");
        assert!(
            d.is_empty(),
            "a *Tester fixture runner imported from a package must be allowed at top level: {d:?}"
        );
    }

    #[test]
    fn allows_named_plugin_tester_fixture_runner_at_top_level() {
        let src = r#"
import { pluginTester } from 'babel-plugin-tester'
import plugin from '../src'

pluginTester({
  plugin,
  fixtures: __dirname,
})
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "fixtures.test.js");
        assert!(
            d.is_empty(),
            "a named *Tester fixture runner imported from a package must be allowed: {d:?}"
        );
    }

    #[test]
    fn flags_tester_named_call_imported_from_relative_path() {
        let src = r#"
import myTester from './helpers'

myTester(__dirname)

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "a *Tester call bound to a relative import must still be flagged (not a published runner): {d:?}"
        );
    }

    #[test]
    fn flags_non_tester_package_call_at_top_level() {
        let src = r#"
import setup from 'some-pkg'

setup()

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert_eq!(
            d.len(),
            1,
            "a package import call that is not a *Tester runner must still be flagged: {d:?}"
        );
    }

    #[test]
    fn allows_declare_global_namespace_augmentation() {
        let src = r#"
declare global {
  namespace PlaywrightTest {
    interface Matchers<R> {
      toRoughlyEqual(expected: number, deviation: number): R
    }
  }
}

describe("x", () => { it("works", () => {}); });
"#;
        let d = crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts");
        assert!(
            d.is_empty(),
            "declare global namespace augmentation must be allowed at top level: {d:?}"
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
        let package_bindings = package_import_bindings(program);
        let mut diagnostics = Vec::new();

        for stmt in &program.body {
            if top_level_is_allowed(stmt, node_test_mode, &package_bindings) {
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
