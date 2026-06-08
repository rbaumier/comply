//! Flags arrow functions whose declared return type is an
//! `asserts` type predicate.

use crate::diagnostic::{Diagnostic, Severity};

fn return_type_has_asserts(arrow: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(rt) = arrow.child_by_field_name("return_type") else {
        return false;
    };
    let text = std::str::from_utf8(&source[rt.byte_range()]).unwrap_or("");
    // `: asserts x is T` or `: asserts x`
    text.contains("asserts ")
}

crate::ast_check! { on ["arrow_function"] prefilter = ["asserts"] => |node, source, ctx, diagnostics|
    if !return_type_has_asserts(node, source) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Assertion functions (`asserts`) must be declared with `function`, not as an arrow.".into(),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_arrow_with_asserts_predicate() {
        let src = "const assertIsString = (x: unknown): asserts x is string => { if (typeof x !== 'string') throw 0; };";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_arrow_with_bare_asserts() {
        let src = "const check = (x: unknown): asserts x => { if (!x) throw 0; };";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_function_declaration_with_asserts() {
        let src = "function assertIsString(x: unknown): asserts x is string { if (typeof x !== 'string') throw 0; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_regular_arrow() {
        let src = "const f = (x: number): string => String(x);";
        assert!(run(src).is_empty());
    }
}
