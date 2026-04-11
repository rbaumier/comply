//! react-forward-ref-uses-ref AST backend.
//!
//! Flags `React.forwardRef((props, ref) => ...)` when the callback has
//! fewer than 2 parameters (no `ref` param).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }

    // Check if it's forwardRef or React.forwardRef.
    let Some(callee) = node.child(0) else { return };
    let Ok(callee_text) = callee.utf8_text(source) else { return };

    if callee_text != "forwardRef" && callee_text != "React.forwardRef" {
        return;
    }

    // Get the first argument (the render function).
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let callback = args.children(&mut cursor).find(|c| {
        c.kind() == "arrow_function" || c.kind() == "function_expression" || c.kind() == "function"
    });

    let Some(callback) = callback else { return };

    // Count parameters.
    let Some(params) = callback.child_by_field_name("parameters") else {
        // No params at all — definitely missing ref.
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-forward-ref-uses-ref".into(),
            message: "`forwardRef` component is missing the `ref` parameter."
                .into(),
            severity: Severity::Warning,
        });
        return;
    };

    // For arrow functions, params can be a single identifier (no parens).
    let param_count = if params.kind() == "formal_parameters" {
        let mut pc = params.walk();
        params.children(&mut pc).filter(|c| {
            c.kind() != "(" && c.kind() != ")" && c.kind() != ","
        }).count()
    } else {
        // Single identifier param.
        1
    };

    if param_count < 2 {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-forward-ref-uses-ref".into(),
            message: "`forwardRef` component is missing the `ref` parameter."
                .into(),
            severity: Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_missing_ref_param() {
        let src = "const Comp = React.forwardRef((props) => <div />);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_param() {
        let src = "const Comp = React.forwardRef((props, ref) => <div ref={ref} />);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_no_params() {
        let src = "const Comp = React.forwardRef(() => <div />);";
        assert_eq!(run(src).len(), 1);
    }
}
