//! react-forward-ref-uses-ref AST backend.
//!
//! Flags `React.forwardRef((props, ref) => ...)` when the callback has
//! fewer than 2 parameters (no `ref` param).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
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
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-forward-ref-uses-ref".into(),
            message: "`forwardRef` component is missing the `ref` parameter."
                .into(),
            severity: Severity::Warning,
            span: None,
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
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-forward-ref-uses-ref".into(),
            message: "`forwardRef` component is missing the `ref` parameter."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
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
