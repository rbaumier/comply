//! no-named-export backend — forbid `export` statements that are not `export default`.
//!
//! Matches tree-sitter's `export_statement`. A default export's text starts
//! with `export default`; anything else (named declaration, named clause,
//! re-export) is a named export and is flagged.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "export_statement" {
        return;
    }
    let text = match node.utf8_text(source) {
        Ok(t) => t,
        Err(_) => return,
    };
    let trimmed = text.trim_start();
    if trimmed.starts_with("export default ") || trimmed.starts_with("export default\n") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-named-export".into(),
        message: "Named exports are forbidden — use `export default` instead.".into(),
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
    fn flags_named_const_export() {
        assert_eq!(run_on("export const x = 1;").len(), 1);
    }

    #[test]
    fn flags_named_function_export() {
        assert_eq!(run_on("export function foo() {}").len(), 1);
    }

    #[test]
    fn flags_re_export() {
        assert_eq!(run_on("export { foo } from './m';").len(), 1);
    }

    #[test]
    fn allows_default_export() {
        assert!(run_on("export default function foo() {}").is_empty());
    }

    #[test]
    fn allows_default_value_export() {
        assert!(run_on("const x = 1; export default x;").is_empty());
    }
}
