//! require-hook AST backend — in test files, flag top-level statements
//! that have side effects (function calls, assignments) which belong
//! inside a `beforeEach` / `beforeAll` hook so tests can control them.

use crate::diagnostic::{Diagnostic, Severity};

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
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
fn root_callee_name<'a>(call: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut cur = call.child_by_field_name("function")?;
    loop {
        match cur.kind() {
            "identifier" | "property_identifier" => {
                return cur.utf8_text(source).ok();
            }
            "member_expression" => {
                cur = cur.child_by_field_name("object")?;
            }
            "call_expression" => {
                cur = cur.child_by_field_name("function")?;
            }
            _ => return None,
        }
    }
}

/// Calls that *must* live at module scope because the test runner hoists
/// or otherwise binds them to file-level evaluation. Moving them inside
/// a hook breaks the framework contract.
///
/// - `vi.mock` / `vi.unmock` / `vi.hoisted` are hoisted to the top by
///   Vitest.
/// - `jest.mock` / `jest.unmock` are hoisted to the top by Jest's babel
///   transform.
/// - `vi.stubEnv` / `vi.stubGlobal` are valid at module scope; pairing
///   rules cover their cleanup.
fn is_hoisted_test_api(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(callee) = call.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(obj) = callee.child_by_field_name("object") else {
        return false;
    };
    let Some(prop) = callee.child_by_field_name("property") else {
        return false;
    };
    if obj.kind() != "identifier" {
        return false;
    }
    let obj_name = obj.utf8_text(source).unwrap_or("");
    let prop_name = prop.utf8_text(source).unwrap_or("");
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

/// Is this initializer pure enough to allow at module scope?
/// Literals, identifiers, template strings without expressions,
/// plain object/array literals, arrow/function expressions, and the
/// `as` cast of any of the above are considered safe.
///
/// Hoisted test framework APIs (e.g. `vi.hoisted(() => ...)`) are also
/// allowed because the runtime lifts them above all imports.
fn is_pure_initializer(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "call_expression" && is_hoisted_test_api(node, source) {
        return true;
    }
    if is_commonjs_import(node, source) {
        return true;
    }
    match node.kind() {
        "string"
        | "number"
        | "true"
        | "false"
        | "null"
        | "undefined"
        | "regex"
        | "identifier"
        | "arrow_function"
        | "function"
        | "function_expression"
        | "class"
        | "class_expression" => true,
        "template_string" => {
            // Template with no ${...} substitutions is a plain literal.
            let mut cur = node.walk();
            !node
                .children(&mut cur)
                .any(|c| c.kind() == "template_substitution")
        }
        "array" => {
            let mut cur = node.walk();
            node.named_children(&mut cur)
                .all(|c| is_pure_initializer(c, source))
        }
        "object" => {
            let mut cur = node.walk();
            node.named_children(&mut cur).all(|c| match c.kind() {
                "pair" => c
                    .child_by_field_name("value")
                    .is_some_and(|v| is_pure_initializer(v, source)),
                "shorthand_property_identifier" | "spread_element" => false,
                _ => false,
            })
        }
        "unary_expression"
        | "parenthesized_expression"
        | "as_expression"
        | "type_assertion"
        | "satisfies_expression"
        | "non_null_expression" => node
            .named_child(0)
            .is_some_and(|c| is_pure_initializer(c, source)),
        _ => false,
    }
}

fn is_commonjs_import(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "identifier" || callee.utf8_text(source).unwrap_or("") != "require" {
        return false;
    }
    let Some(args) = node.child_by_field_name("arguments") else {
        return false;
    };
    let mut cur = args.walk();
    let mut named = args.named_children(&mut cur);
    let Some(first) = named.next() else {
        return false;
    };
    first.kind() == "string" && named.next().is_none()
}

/// Every declarator in a `const`/`let`/`var` statement must have a
/// pure initializer (or none at all).
fn declaration_is_pure(decl: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = decl.walk();
    for child in decl.named_children(&mut cur) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        if let Some(value) = child.child_by_field_name("value")
            && !is_pure_initializer(value, source)
        {
            return false;
        }
    }
    true
}

/// Classify a top-level statement: `true` means it's allowed, `false`
/// means it's a side-effect statement that should move inside a hook.
fn top_level_is_allowed(stmt: tree_sitter::Node, source: &[u8]) -> bool {
    match stmt.kind() {
        // Module structure and declarations.
        "import_statement"
        | "export_statement"
        | "function_declaration"
        | "generator_function_declaration"
        | "class_declaration"
        | "interface_declaration"
        | "type_alias_declaration"
        | "enum_declaration"
        | "module"
        | "namespace_declaration"
        | "internal_module"
        | "ambient_declaration"
        | "empty_statement"
        | "comment" => true,

        "lexical_declaration" | "variable_declaration" => declaration_is_pure(stmt, source),

        // `describe(...)`, `test(...)`, `beforeEach(...)`, etc., plus
        // hoisted framework APIs (`vi.mock`, `jest.mock`, `vi.hoisted`,
        // `vi.stubEnv`, `vi.stubGlobal`) that must live at module scope.
        "expression_statement" => {
            let Some(expr) = stmt.named_child(0) else {
                return false;
            };
            if expr.kind() == "string" {
                return true;
            }
            if expr.kind() != "call_expression" {
                return false;
            }
            if is_hoisted_test_api(expr, source) {
                return true;
            }
            let Some(name) = root_callee_name(expr, source) else {
                return false;
            };
            is_allowed_test_callee(name)
        }

        _ => false,
    }
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    let mut cur = node.walk();
    for stmt in node.named_children(&mut cur) {
        if top_level_is_allowed(stmt, source) {
            continue;
        }
        let pos = stmt.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "require-hook".into(),
            message:
                "Top-level side effect in a test file — move it into a `beforeEach` or `beforeAll` hook."
                    .into(),
            severity: Severity::Warning,
            span: Some((stmt.start_byte(), stmt.end_byte() - stmt.start_byte())),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let ctx =
            crate::rules::backend::CheckCtx::for_test(std::path::Path::new("foo.test.ts"), source);
        use crate::rules::backend::AstCheck;
        Check.check(&ctx, &tree)
    }

    fn run_on_non_test(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let ctx = crate::rules::backend::CheckCtx::for_test(std::path::Path::new("foo.ts"), source);
        use crate::rules::backend::AstCheck;
        Check.check(&ctx, &tree)
    }

    #[test]
    fn flags_top_level_function_call() {
        let src = r#"
import { setup } from "./helpers";
setup();
describe("x", () => {
  it("works", () => { expect(1).toBe(1); });
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_side_effect_inside_before_each() {
        let src = r#"
import { setup } from "./helpers";
describe("x", () => {
  beforeEach(() => { setup(); });
  it("works", () => { expect(1).toBe(1); });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_const_with_literal_initializer() {
        let src = r#"
const N = 42;
const name = "fixture";
const values = [1, 2, 3];
describe("x", () => {
  it("works", () => { expect(N).toBe(42); });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_const_with_call_initializer() {
        let src = r#"
const user = buildUser();
describe("x", () => {
  it("works", () => { expect(user).toBeDefined(); });
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_commonjs_require_imports_at_top_level() {
        let src = r#"
'use strict'
const { test } = require('node:test');
const { Readable } = require('node:stream');
const Fastify = require('..');

test('works', () => {});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_describe_and_hooks_at_top_level() {
        let src = r#"
beforeAll(() => { connect(); });
afterAll(() => { disconnect(); });
describe("suite", () => {
  it("works", () => { expect(true).toBe(true); });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_function_and_class_declarations() {
        let src = r#"
function makeUser() { return { id: 1 }; }
class Fixture {}
describe("x", () => {
  it("works", () => { expect(makeUser().id).toBe(1); });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_top_level_assignment() {
        let src = r#"
let counter = 0;
counter = 5;
describe("x", () => {
  it("works", () => { expect(counter).toBe(5); });
});
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_non_test_files() {
        let src = r#"
setup();
const user = buildUser();
"#;
        assert!(run_on_non_test(src).is_empty());
    }

    #[test]
    fn allows_describe_only_and_it_each() {
        let src = r#"
describe.skip("suite", () => {
  it.each([1, 2])("case %s", (n) => { expect(n).toBeGreaterThan(0); });
});
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_vi_mock_at_top_level() {
        let src = r#"
import { vi, describe, it, expect } from "vitest";
vi.mock("./db", () => ({ query: vi.fn() }));
vi.unmock("./cache");
describe("x", () => { it("works", () => { expect(1).toBe(1); }); });
"#;
        let d = run_on(src);
        assert!(
            d.is_empty(),
            "vi.mock/unmock must be allowed at top level: {d:?}"
        );
    }

    #[test]
    fn allows_jest_mock_at_top_level() {
        let src = r#"
jest.mock("./db");
jest.unmock("./cache");
describe("x", () => { it("works", () => { expect(1).toBe(1); }); });
"#;
        let d = run_on(src);
        assert!(
            d.is_empty(),
            "jest.mock/unmock must be allowed at top level: {d:?}"
        );
    }

    #[test]
    fn allows_vi_hoisted_at_top_level() {
        let src = r#"
import { vi } from "vitest";
const { mocks } = vi.hoisted(() => ({ mocks: { fn: vi.fn() } }));
vi.mock("./mod", () => ({ fn: mocks.fn }));
"#;
        let d = run_on(src);
        assert!(
            d.is_empty(),
            "vi.hoisted (call + lexical decl from pure call) must be allowed: {d:?}"
        );
    }

    #[test]
    fn allows_vi_stub_env_and_global_at_top_level() {
        let src = r#"
import { vi } from "vitest";
vi.stubEnv("NODE_ENV", "test");
vi.stubGlobal("fetch", vi.fn());
"#;
        let d = run_on(src);
        assert!(
            d.is_empty(),
            "vi.stubEnv / vi.stubGlobal must be allowed at top level: {d:?}"
        );
    }
}
