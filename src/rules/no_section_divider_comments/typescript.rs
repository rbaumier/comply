//! no-section-divider-comments — TS/JS/TSX backend.
//!
//! Walks `comment` AST nodes and flags those whose body contains a long
//! run of divider characters (`=`, `-`, `*`, `#`, `~`).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["comment"] => |node, source, ctx, diagnostics|
    let Ok(text) = node.utf8_text(source) else { return; };
    let min_run = ctx.config.threshold("no-section-divider-comments", "min_run");
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
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_equals_divider() {
        assert_eq!(run("// ============").len(), 1);
    }

    #[test]
    fn flags_dashes_divider() {
        assert_eq!(run("// ----- SETUP -----").len(), 1);
    }

    #[test]
    fn flags_stars_divider() {
        assert_eq!(run("// ***** PRIVATE *****").len(), 1);
    }

    #[test]
    fn allows_short_dashes() {
        assert!(run("// -- note").is_empty());
    }

    #[test]
    fn allows_normal_comment() {
        assert!(run("// Apply the cursor advance after commit").is_empty());
    }

    #[test]
    fn ignores_dividers_in_code() {
        assert!(run("const x = '====================';").is_empty());
    }

    #[test]
    fn flags_block_comment_divider() {
        assert_eq!(run("/* ============== */").len(), 1);
    }
}
