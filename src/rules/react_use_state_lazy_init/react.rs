//! react-use-state-lazy-init backend — flag `useState(fn())` where the
//! argument is a non-trivial function call.
//!
//! Why: `useState(getInitial())` evaluates `getInitial()` on every render,
//! not the first. The value is thrown away after mount but the cost
//! stays. Worse, `useState(window.innerWidth)` crashes in SSR. The lazy
//! form `useState(() => getInitial())` evaluates only once.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if name != "useState" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let Some(first_arg) = args.named_child(0) else {
        return;
    };
    // Flag function calls and member expressions (window.innerWidth etc.).
    if !matches!(first_arg.kind(), "call_expression" | "member_expression") {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-use-state-lazy-init".into(),
        message: "`useState(expensive())` runs the initializer on every render \
                  and crashes in SSR. Wrap in a lazy function: \
                  `useState(() => expensive())`."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_use_state_with_function_call() {
        assert_eq!(run_on("const [w] = useState(getInitial());").len(), 1);
    }

    #[test]
    fn flags_use_state_with_browser_api() {
        assert_eq!(run_on("const [w] = useState(window.innerWidth);").len(), 1);
    }

    #[test]
    fn allows_lazy_init() {
        assert!(run_on("const [w] = useState(() => getInitial());").is_empty());
    }

    #[test]
    fn allows_primitive_init() {
        assert!(run_on("const [w] = useState(0);").is_empty());
        assert!(run_on("const [w] = useState('');").is_empty());
    }
}
