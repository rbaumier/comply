//! import-no-named-export backend — forbid named exports.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();

    // Flag `export { ... }` and `export const/function/class ...` (named exports).
    // Allow `export default`.
    if kind == "export_statement" {
        let text = node.utf8_text(source).unwrap_or("");

        // `export default` is fine.
        if text.starts_with("export default") {
            return;
        }

        // `export { default }` re-export is fine.
        if text.contains("{ default }") || text.contains("{ default as") {
            return;
        }

        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "import-no-named-export".into(),
            message: "Named exports are not allowed.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_named_export() {
        let d = run_on("export const foo = 1;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Named exports"));
    }

    #[test]
    fn allows_default_export() {
        assert!(run_on("export default function foo() {}").is_empty());
    }

    #[test]
    fn flags_named_function_export() {
        let d = run_on("export function bar() {}");
        assert_eq!(d.len(), 1);
    }
}
