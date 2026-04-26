//! testing-prefer-test-each backend — flag `for` / `forEach` loops that
//! wrap a `test` / `it` / `test(...)` call.
//!
//! Why: when the loop body throws, the framework reports a single failure
//! somewhere inside the loop instead of naming which row broke.
//! `test.each(cases)(name, fn)` creates one case per row, each with its
//! own name and its own failure report.
//!
//! Detection: any `for` / `for_in` / `for_of` / `while` statement whose
//! body contains a top-level call to `test(...)`, `it(...)`,
//! `test.only(...)`, `it.skip(...)` etc., or an array `.forEach(fn)`
//! whose callback body calls the same.

use crate::diagnostic::{Diagnostic, Severity};

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

/// `true` if this call-expression's callee is `test` / `it` or
/// `test.only` / `it.skip` / ….
fn is_test_call(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(func) = node.child_by_field_name("function") else {
        return false;
    };
    match func.kind() {
        "identifier" => {
            let name = func.utf8_text(source).unwrap_or("");
            matches!(name, "test" | "it")
        }
        "member_expression" => {
            let Some(obj) = func.child_by_field_name("object") else {
                return false;
            };
            if obj.kind() != "identifier" {
                return false;
            }
            let name = obj.utf8_text(source).unwrap_or("");
            matches!(name, "test" | "it")
        }
        _ => false,
    }
}

/// Walk every descendant of `root` and return `true` if any of them is a
/// test call (stopping early at the first match).
fn contains_test_call(root: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if is_test_call(n, source) {
            return true;
        }
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }
    false
}

fn push(diagnostics: &mut Vec<Diagnostic>, ctx: &crate::rules::backend::CheckCtx, node: tree_sitter::Node, kind: &str) {
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "testing-prefer-test-each".into(),
        message: format!(
            "`{kind}` wraps a `test` / `it` call — replace the loop with `test.each(cases)(...)` so each row is a separate named case."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

crate::ast_check! { on ["for_statement", "for_in_statement", "while_statement", "call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) { return; }
match node.kind() {
        "for_statement" | "for_in_statement" | "while_statement" => {
            let Some(body) = node.child_by_field_name("body") else { return };
            if contains_test_call(body, source) {
                push(diagnostics, ctx, node, node.kind().trim_end_matches("_statement"));
            }
        }
        "call_expression" => {
            // Match `xs.forEach(cb)` where `cb` body has a test call.
            let Some(func) = node.child_by_field_name("function") else { return };
            if func.kind() != "member_expression" { return; }
            let Some(prop) = func.child_by_field_name("property") else { return };
            if prop.utf8_text(source).unwrap_or("") != "forEach" { return; }
            let Some(args) = node.child_by_field_name("arguments") else { return };
            let Some(cb) = args.named_child(0) else { return };
            if !matches!(cb.kind(), "arrow_function" | "function_expression" | "function") {
                return;
            }
            let Some(body) = cb.child_by_field_name("body") else { return };
            if contains_test_call(body, source) {
                push(diagnostics, ctx, node, "forEach");
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new("f.test.ts"), source), &tree)
    }

    fn run_non_test(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new("f.ts"), source), &tree)
    }

    #[test]
    fn flags_for_of_with_test_call() {
        let src = r#"
for (const c of cases) {
  test(c.name, () => { expect(c.input).toBe(c.expected) });
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_foreach_with_it_call() {
        let src = r#"
cases.forEach(c => {
  it(c.name, () => { expect(c.input).toBe(c.expected) });
});
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_for_in_with_test_call() {
        let src = r#"
for (const k in cases) {
  test(k, () => {});
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_test_each() {
        let src = "test.each(cases)('%s', (c) => { expect(c.input).toBe(c.expected) });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_loop_without_test_call() {
        let src = r#"
for (const c of cases) {
  const result = transform(c);
  cache.push(result);
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let src = r#"
for (const c of cases) {
  test(c.name, () => {});
}
"#;
        assert!(run_non_test(src).is_empty());
    }
}
