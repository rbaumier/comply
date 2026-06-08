//! no-abusive-eslint-disable — Rust backend.
//!
//! Rust source files don't normally contain eslint directives, but
//! comply still scans them — match the existing TextCheck coverage so
//! switching backends is behaviour-preserving.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["line_comment", "block_comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    if !super::is_abusive_disable(text) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Specify the rules you want to disable.".into(),
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_bare_disable_in_rust_comment() {
        assert_eq!(run("// eslint-disable-next-line\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_clean_comment() {
        assert!(run("// just a normal comment\nfn f() {}").is_empty());
    }
}
