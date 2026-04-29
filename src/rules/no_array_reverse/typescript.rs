//! no-array-reverse backend — flag `.reverse()` calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = ["reverse"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if method != "reverse" {
        return;
    }

    // Ensure it's a zero-argument call (no arguments besides the callee).
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() != 0 {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-array-reverse".into(),
        message: "`Array#reverse()` mutates in place — use `.toReversed()` to avoid mutation.".into(),
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
    fn flags_reverse() {
        assert_eq!(run_on("const rev = arr.reverse();").len(), 1);
    }

    #[test]
    fn flags_chained_reverse() {
        assert_eq!(run_on("arr.filter(x => x > 0).reverse();").len(), 1);
    }

    #[test]
    fn allows_to_reversed() {
        assert!(run_on("const rev = arr.toReversed();").is_empty());
    }

    #[test]
    fn allows_unrelated() {
        assert!(run_on("const x = arr.map(x => x * 2);").is_empty());
    }
}
