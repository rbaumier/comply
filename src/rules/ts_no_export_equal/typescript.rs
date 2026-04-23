//! ts-no-export-equal backend — flag CommonJS-style `export = X` in TypeScript.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, _source, ctx, diagnostics|
    if node.kind() != "export_statement" {
        return;
    }
    // `export = X;` has a direct `=` child. `export default X` has `default`.
    // Regular ES exports (`export const`, `export { a }`, `export * from …`) have
    // neither.
    let mut cursor = node.walk();
    let mut has_equals = false;
    for child in node.children(&mut cursor) {
        if child.kind() == "=" {
            has_equals = true;
            break;
        }
    }
    if !has_equals {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "ts-no-export-equal".into(),
        message: "CommonJS-style `export = ...` — use `export default` or named exports.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_export_equal_value() {
        let d = run_on("const x = 1;\nexport = x;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("export = "));
    }

    #[test]
    fn flags_export_equal_class() {
        let d = run_on("class Foo {}\nexport = Foo;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_export_default() {
        assert!(run_on("const x = 1;\nexport default x;").is_empty());
    }

    #[test]
    fn allows_named_export() {
        assert!(run_on("export const x = 1;").is_empty());
    }
}
