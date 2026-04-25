//! no-abusive-eslint-disable — Rust backend.
//!
//! Rust source files don't normally contain eslint directives, but
//! comply still scans them — match the existing TextCheck coverage so
//! switching backends is behaviour-preserving.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if !matches!(node.kind(), "line_comment" | "block_comment") { return; }
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
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
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
