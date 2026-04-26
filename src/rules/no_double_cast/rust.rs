//! no-double-cast Rust backend — flag `x as u32 as u64` chained casts.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["type_cast_expression"] => |node, _source, ctx, diagnostics|
    // The inner expression (left side of `as`) is the first named child.
    let Some(inner) = node.child_by_field_name("value") else { return };
    if inner.kind() != "type_cast_expression" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-double-cast".into(),
        message: "Double cast `as X as Y` hides misaligned types. \
                  Fix the real problem: align the types or use `From`/`Into`.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_double_as_cast() {
        assert_eq!(run_on("fn f(x: i8) { let _ = x as u32 as u64; }").len(), 1);
    }

    #[test]
    fn allows_single_cast() {
        assert!(run_on("fn f(x: i32) { let _ = x as u64; }").is_empty());
    }

    #[test]
    fn flags_triple_cast() {
        let d = run_on("fn f(x: i8) { let _ = x as i16 as u32 as u64; }");
        // The outer cast and the middle cast are both flagged.
        assert!(!d.is_empty());
    }
}
