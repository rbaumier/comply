//! no-section-divider-comments — Rust backend.
//!
//! Walks `line_comment` and `block_comment` AST nodes and flags those
//! whose body contains a long run of divider characters.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["line_comment", "block_comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    let min_run = ctx.config.threshold("no-section-divider-comments", "min_run", ctx.lang);
    if !super::is_section_divider_text(text, min_run) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Section divider comment — signal that the file is doing \
         too many things. Split the file by responsibility instead \
         of decorating the boundary with `===` or `***`."
            .into(),
        Severity::Error,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_equals_divider() {
        assert_eq!(run("// ============\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_short_dashes() {
        assert!(run("// -- note\nfn f() {}").is_empty());
    }

    #[test]
    fn ignores_dividers_in_code() {
        assert!(run("fn f() { let x = \"====================\"; }").is_empty());
    }
}
