//! no-let backend — flag `let` declarations.
//!
//! `lexical_declaration` covers both `let` and `const`. The first child
//! of the node is the keyword token (`let` or `const`), so we check its
//! text directly rather than the wider declaration text (which can
//! contain `let` inside identifiers or strings on the RHS).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["lexical_declaration"] prefilter = ["let"] => |node, source, ctx, diagnostics|
    let Some(first) = node.child(0) else { return };
    let Ok(text) = first.utf8_text(source) else { return };
    if text != "let" {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-let",
        "`let` creates a mutable binding — use `const` instead.".into(),
        Severity::Warning,
    ));
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_let_declaration() {
        assert_eq!(run_on("let x = 1;").len(), 1);
    }

    #[test]
    fn flags_let_with_type_annotation() {
        assert_eq!(run_on("let x: number = 1;").len(), 1);
    }

    #[test]
    fn allows_const_declaration() {
        assert!(run_on("const x = 1;").is_empty());
    }

    #[test]
    fn ignores_var_declaration() {
        // `var` is a variable_declaration node, not lexical_declaration.
        assert!(run_on("var x = 1;").is_empty());
    }
}
