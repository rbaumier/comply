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

/// Is this initializer pure enough to allow at module scope?
/// Literals, identifiers, template strings without expressions,
/// plain object/array literals, arrow/function expressions, and the
/// `as` cast of any of the above are considered safe.
fn is_pure_initializer(node: tree_sitter::Node) -> bool {
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
            node.named_children(&mut cur).all(|c| is_pure_initializer(c))
        }
        "object" => {
            let mut cur = node.walk();
            node.named_children(&mut cur).all(|c| match c.kind() {
                "pair" => c
                    .child_by_field_name("value")
                    .is_some_and(|v| is_pure_initializer(v)),
                "shorthand_property_identifier" | "spread_element" => false,
                _ => false,
            })
        }
        "unary_expression" | "parenthesized_expression" | "as_expression"
        | "type_assertion" | "satisfies_expression" | "non_null_expression" => {
            node.named_child(0).is_some_and(|c| is_pure_initializer(c))
        }
        _ => false,
    }
}

/// Every declarator in a `const`/`let`/`var` statement must have a
/// pure initializer (or none at all).
fn declaration_is_pure(decl: tree_sitter::Node) -> bool {
    let mut cur = decl.walk();
    for child in decl.named_children(&mut cur) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        if let Some(value) = child.child_by_field_name("value")
            && !is_pure_initializer(value)
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

        "lexical_declaration" | "variable_declaration" => declaration_is_pure(stmt),

        // `describe(...)`, `test(...)`, `beforeEach(...)`, etc.
        "expression_statement" => {
            let Some(expr) = stmt.named_child(0) else {
                return false;
            };
            if expr.kind() != "call_expression" {
                return false;
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
        let ctx = crate::rules::backend::CheckCtx::for_test(
            std::path::Path::new("foo.test.ts"),
            source,
        );
        use crate::rules::backend::AstCheck;
        Check.check(&ctx, &tree)
    }

    fn run_on_non_test(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let ctx = crate::rules::backend::CheckCtx::for_test(
            std::path::Path::new("foo.ts"),
            source,
        );
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
}
