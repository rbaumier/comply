//! no-hidden-control-flow backend — flag 3+ stacked decorators.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    // Look for class/function declarations that can have decorators.
    match node.kind() {
        "class_declaration" | "method_definition" | "export_statement" => {}
        _ => return,
    }

    // Count consecutive decorator children.
    let mut count = 0u32;
    let child_count = node.named_child_count();
    for i in 0..child_count {
        let Some(child) = node.named_child(i) else { continue };
        if child.kind() == "decorator" {
            count += 1;
        }
    }

    if count >= 3 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-hidden-control-flow".into(),
            message: format!(
                "{count} stacked decorators hide control flow — compose into fewer decorators or use explicit middleware."
            ),
            severity: Severity::Warning,
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
    fn flags_three_decorators() {
        let src = "@Auth()\n@Log()\n@Cache()\nclass MyService {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_two_decorators() {
        let src = "@Auth()\n@Log()\nclass MyService {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_one_decorator() {
        let src = "@Injectable()\nclass Service {}";
        assert!(run_on(src).is_empty());
    }
}
