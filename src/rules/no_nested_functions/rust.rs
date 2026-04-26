//! no-nested-functions Rust backend — flag `fn` nested 3+ levels deep.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["function_item"] => |node, _source, ctx, diagnostics|
    // Walk ancestors to count function nesting depth.
    let mut depth = 0usize;
    let mut parent = node.parent();
    while let Some(p) = parent {
        if p.kind() == "function_item" {
            depth += 1;
        }
        parent = p.parent();
    }
    if depth >= 2 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-nested-functions".into(),
            message: format!(
                "Function declared at nesting depth {} \u{2014} extract to module scope.",
                depth
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_deeply_nested_function() {
        let src = r#"fn outer() {
    fn middle() {
        fn too_deep() {
            return;
        }
    }
}"#;
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-nested-functions");
        assert!(d[0].message.contains("depth 2"));
    }

    #[test]
    fn allows_two_levels() {
        let src = r#"fn outer() {
    fn inner() {
        return;
    }
}"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_top_level_function() {
        let src = "fn foo() { return; }";
        assert!(run_on(src).is_empty());
    }
}
