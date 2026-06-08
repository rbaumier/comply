//! no-throw backend for TypeScript / JavaScript / TSX.
//!
//! Walks the AST looking for every `throw_statement` node and emits one
//! diagnostic per occurrence. Covers all three TS-family grammars since
//! tree-sitter-typescript parses plain JS too; TSX uses its own grammar
//! variant but exposes the same `throw_statement` kind.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["throw_statement"] prefilter = ["throw"] => |node, _source, ctx, diagnostics|
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-throw".into(),
        message: "Use Result<T, E> instead of throw — surface errors as values, \
                  not exceptions. Callers can't see thrown errors in the type signature."
            .into(),
        severity: Severity::Error,
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_throw_statement() {
        let diags = run_on("function f() { throw new Error('boom'); }");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-throw");
    }

    #[test]
    fn allows_code_without_throw() {
        assert!(run_on("function f() { return 42; }").is_empty());
    }

    #[test]
    fn flags_multiple_throws() {
        let diags = run_on("function f() { throw 1; } function g() { throw 2; }");
        assert_eq!(diags.len(), 2);
    }
}
