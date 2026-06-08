//! no-unthrown-error AST backend — `new Error(...)` created but never used.
//!
//! Walks `new_expression` nodes whose constructor is `Error`. Flags only
//! when the expression sits as a top-level expression statement
//! (`new Error(...);`) — meaning the value isn't thrown, returned,
//! assigned, or otherwise consumed.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    let Some(ctor) = node.child_by_field_name("constructor") else { return };
    let ctor_name = ctor.utf8_text(source).unwrap_or("");
    if ctor_name != "Error" {
        return;
    }

    // Only flag if the immediate parent is an expression_statement —
    // i.e. the freshly built error is dropped on the floor.
    let Some(parent) = node.parent() else { return };
    if parent.kind() != "expression_statement" {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "no-unthrown-error",
        "`new Error(...)` is created but never thrown — add `throw` or assign the error.".into(),
        Severity::Error,
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
    fn flags_unthrown_error() {
        assert_eq!(run_on("  new Error(\"oops\");").len(), 1);
    }

    #[test]
    fn flags_bare_new_error() {
        assert_eq!(run_on("new Error(\"something went wrong\");").len(), 1);
    }

    #[test]
    fn allows_thrown_error() {
        assert!(run_on("throw new Error(\"oops\");").is_empty());
    }

    #[test]
    fn allows_assigned_error() {
        assert!(run_on("const err = new Error(\"oops\");").is_empty());
    }

    #[test]
    fn allows_returned_error() {
        assert!(run_on("return new Error(\"oops\");").is_empty());
    }
}
