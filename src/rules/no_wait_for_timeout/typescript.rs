//! no-wait-for-timeout AST backend — flag `<receiver>.waitForTimeout(...)`
//! invocations in test files. Walks `call_expression` nodes whose
//! callee is a `member_expression` with property `waitForTimeout`.

use crate::diagnostic::{Diagnostic, Severity};

/// Returns `true` when the file path looks like a test file.
fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.")
        || s.contains(".spec.")
        || s.contains("__tests__")
        || s.contains("_test.")
        || s.contains(".e2e.")
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_test_file(ctx.path) {
        return;
    }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" {
        return;
    }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let name = std::str::from_utf8(&source[prop.byte_range()]).unwrap_or("");
    if name != "waitForTimeout" {
        return;
    }
    // Anchor diagnostic on the `waitForTimeout` identifier so the
    // (line, column) matches the property name (matches legacy text-based
    // behaviour expected by `correct_line_and_column`).
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &prop,
        super::META.id,
        "`waitForTimeout` is a fixed sleep — replace with a \
         web-first assertion or `waitForResponse`."
            .into(),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::rules::backend::{AstCheck, CheckCtx};

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .expect("grammar should load");
        let tree = parser.parse(source, None).expect("parser should produce a tree");
        Check.check(&CheckCtx::for_test(Path::new(path), source), &tree)
    }

    #[test]
    fn flags_wait_for_timeout_in_test() {
        let diags = run(
            "login.test.ts",
            "await page.waitForTimeout(1000);",
        );
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-wait-for-timeout");
    }

    #[test]
    fn flags_in_spec_file() {
        let diags = run("checkout.spec.ts", "  await page.waitForTimeout(500);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_in_e2e_file() {
        let diags = run("smoke.e2e.ts", "await page.waitForTimeout(2000);");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_wait_for_response() {
        let diags = run(
            "api.test.ts",
            "await page.waitForResponse('**/api/data');",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn ignores_non_test_file() {
        let diags = run(
            "helpers.ts",
            "await page.waitForTimeout(1000);",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn correct_line_and_column() {
        let source = "const x = 1;\nawait page.waitForTimeout(300);\n";
        let diags = run("foo.test.ts", source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[0].column, 12);
    }
}
