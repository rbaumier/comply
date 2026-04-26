//! no-same-argument-assert backend — asserting a value equals itself.
//!
//! Walks `call_expression` nodes shaped like `expect(<actual>).toBe(<arg>)`
//! or `.toEqual(<arg>)` and flags the call when both arguments have the
//! same textual content. Restricted to test files (`.test.`, `.spec.`,
//! `__tests__`, `_test.`) because the same shape is legitimate elsewhere.

use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__") || s.contains("_test.")
}

/// Return the single named argument inside an `arguments` node, normalized
/// by stripping outer whitespace from its source slice.
fn single_arg_text<'a>(args: Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = args.walk();
    let mut named = args.named_children(&mut cursor);
    let first = named.next()?;
    if named.next().is_some() {
        return None; // exactly one argument required
    }
    let r = first.byte_range();
    std::str::from_utf8(&source[r.start..r.end]).ok().map(str::trim)
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    // Outer call shape: <member_expression>(<arguments>) where the member
    // property is `toBe` or `toEqual` and the object is `expect(<actual>)`.
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let prop_text = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
    if prop_text != "toBe" && prop_text != "toEqual" {
        return;
    }

    let Some(obj) = func.child_by_field_name("object") else { return };
    if obj.kind() != "call_expression" {
        return;
    }
    let Some(expect_callee) = obj.child_by_field_name("function") else { return };
    if expect_callee.kind() != "identifier" {
        return;
    }
    if std::str::from_utf8(&source[expect_callee.byte_range()]).unwrap_or("") != "expect" {
        return;
    }

    let Some(expect_args) = obj.child_by_field_name("arguments") else { return };
    let Some(matcher_args) = node.child_by_field_name("arguments") else { return };

    let Some(actual_text) = single_arg_text(expect_args, source) else { return };
    let Some(expected_text) = single_arg_text(matcher_args, source) else { return };

    if actual_text.is_empty() || actual_text != expected_text {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-same-argument-assert".into(),
        message: "Asserting a value equals itself — this is always true and tests nothing.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run_test_file(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new("foo.test.ts"), source), &tree)
    }

    #[test]
    fn flags_same_arg_tobe() {
        assert_eq!(run_test_file("  expect(x).toBe(x);").len(), 1);
    }

    #[test]
    fn flags_same_arg_to_equal() {
        assert_eq!(run_test_file("  expect(result).toEqual(result);").len(), 1);
    }

    #[test]
    fn allows_different_args() {
        assert!(run_test_file("  expect(actual).toBe(expected);").is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        // run_ts uses "t.ts" which is not a test file.
        assert!(crate::rules::test_helpers::run_ts("  expect(x).toBe(x);", &Check).is_empty());
    }
}
