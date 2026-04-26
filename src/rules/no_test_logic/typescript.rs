//! no-test-logic backend — reject control-flow logic inside test bodies.
//!
//! Walks `call_expression` nodes whose callee identifies a test definition
//! (`it`, `it.each`, `test`, `test.each`) and inspects the test body — the
//! function/arrow argument's `statement_block` — for `if_statement`,
//! `for_statement`, `for_in_statement`, `while_statement`, `do_statement`,
//! and `switch_statement` nodes. Setup hooks (`beforeEach`, `afterEach`,
//! `beforeAll`, `afterAll`) and any nested function bodies are skipped so
//! their control flow is not attributed to the test.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

const TEST_CALLEES: &[&str] = &["it", "test"];

const SETUP_HOOKS: &[&str] = &["beforeEach", "afterEach", "beforeAll", "afterAll"];

const NESTED_FN_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function_declaration",
    "generator_function",
];

const CONTROL_FLOW_KINDS: &[(&str, &str)] = &[
    ("if_statement", "if"),
    ("for_statement", "for"),
    ("for_in_statement", "for"),
    ("while_statement", "while"),
    ("do_statement", "while"),
    ("switch_statement", "switch"),
];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// True if the call expression's callee is a test definition (`it(...)`,
/// `test(...)`, `it.each(...)(...)`, `test.each(...)(...)`).
fn is_test_call(call: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(callee) = call.child_by_field_name("function") else { return false };
    match callee.kind() {
        "identifier" => {
            let Ok(text) = callee.utf8_text(source) else { return false };
            TEST_CALLEES.contains(&text)
        }
        "member_expression" => {
            // `it.skip(...)`, `test.only(...)` — leftmost identifier is the test fn.
            let Some(object) = callee.child_by_field_name("object") else { return false };
            if object.kind() != "identifier" {
                return false;
            }
            let Ok(text) = object.utf8_text(source) else { return false };
            TEST_CALLEES.contains(&text)
        }
        "call_expression" => {
            // `it.each([...])(...)` — recurse into the inner call.
            is_test_call(callee, source)
        }
        _ => false,
    }
}

/// Find the `statement_block` body of the last function/arrow argument
/// passed to a test call.
fn test_body<'t>(call: tree_sitter::Node<'t>) -> Option<tree_sitter::Node<'t>> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let mut last_fn: Option<tree_sitter::Node> = None;
    for arg in args.named_children(&mut cursor) {
        if NESTED_FN_KINDS.contains(&arg.kind()) {
            last_fn = Some(arg);
        }
    }
    let body = last_fn?.child_by_field_name("body")?;
    if body.kind() == "statement_block" {
        Some(body)
    } else {
        None
    }
}

/// Recursively find control-flow nodes within `node`, skipping nested
/// function bodies and setup-hook calls so only the test's own logic is
/// counted.
fn collect_control_flow<'t>(
    node: tree_sitter::Node<'t>,
    source: &[u8],
    out: &mut Vec<(tree_sitter::Node<'t>, &'static str)>,
) {
    let count = node.child_count();
    for i in 0..count {
        let Some(child) = node.child(i) else { continue };
        let kind = child.kind();

        if NESTED_FN_KINDS.contains(&kind) {
            // The hook arrow/function bodies and any other inner function
            // are excluded from the test body's control-flow check.
            continue;
        }

        if kind == "call_expression"
            && let Some(callee) = child.child_by_field_name("function")
            && callee.kind() == "identifier"
            && let Ok(name) = callee.utf8_text(source)
            && SETUP_HOOKS.contains(&name)
        {
            // Setup hooks are allowed to have logic — skip the entire call.
            continue;
        }

        if let Some((_, label)) = CONTROL_FLOW_KINDS.iter().find(|(k, _)| *k == kind) {
            out.push((child, *label));
            // Don't descend; nested control-flow inside an already-flagged
            // node would just produce duplicate diagnostics on the same line.
            continue;
        }

        collect_control_flow(child, source, out);
    }
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    if !is_test_call(node, source) {
        return;
    }
    let Some(body) = test_body(node) else { return };

    let mut hits: Vec<(tree_sitter::Node, &str)> = Vec::new();
    collect_control_flow(body, source, &mut hits);

    for (hit, label) in hits {
        let pos = hit.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-test-logic".into(),
            message: format!(
                "Control-flow `{label}` inside test body — tests should have a single linear assertion path."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run_test_file(path: &str, source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new(path), source), &tree)
    }

    #[test]
    fn flags_if_in_test() {
        let source = "test('x', () => {\n    if (true) {\n        expect(1).toBe(1);\n    }\n});";
        let diags = run_test_file("app/__tests__/foo.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("if"));
    }

    #[test]
    fn flags_for_in_test() {
        let source = "it('does stuff', () => {\n    for (const x of items) {\n        expect(x).toBeDefined();\n    }\n});";
        let diags = run_test_file("src/utils.spec.ts", source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("for"));
    }

    #[test]
    fn ignores_non_test_file() {
        let source = "if (condition) {\n    doSomething();\n}";
        assert!(run_test_file("src/utils.ts", source).is_empty());
    }
}
