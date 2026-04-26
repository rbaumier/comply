//! no-array-constructor backend — flag `new Array()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    let Some(ctor) = node.child_by_field_name("constructor") else { return };
    if ctor.kind() != "identifier" {
        return;
    }
    let name = ctor.utf8_text(source).unwrap_or("");
    if name != "Array" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-array-constructor".into(),
        message: "Avoid `new Array()` — use array literals `[]` instead.".into(),
        severity: Severity::Error,
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
    fn flags_new_array_numeric() {
        assert_eq!(run_on("const a = new Array(3);").len(), 1);
    }

    #[test]
    fn flags_new_array_with_elements() {
        assert_eq!(run_on("const a = new Array(1, 2, 3);").len(), 1);
    }

    #[test]
    fn allows_array_literal() {
        assert!(run_on("const a = [1, 2, 3];").is_empty());
    }

    #[test]
    fn allows_array_from() {
        assert!(run_on("const a = Array.from({ length: 3 });").is_empty());
    }

    #[test]
    fn allows_new_map() {
        assert!(run_on("const m = new Map();").is_empty());
    }
}
