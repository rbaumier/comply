//! no-section-divider-comments — Rust backend.
//!
//! Walks `line_comment` and `block_comment` AST nodes and flags those
//! whose body contains a long run of divider characters.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir {
            return vec![];
        }
        let line_count = ctx.source.bytes().filter(|&b| b == b'\n').count() + 1;
        if line_count < 150 {
            return vec![];
        }
        let min_run = ctx
            .config
            .threshold("no-section-divider-comments", "min_run", ctx.lang);
        let mut diagnostics = Vec::new();
        crate::rules::walker::walk_tree(tree, |node| {
            if !matches!(node.kind(), "line_comment" | "block_comment") {
                return;
            }
            let Ok(text) = node.utf8_text(ctx.source.as_bytes()) else {
                return;
            };
            if !super::is_section_divider_text(text, min_run) {
                return;
            }
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
        });
        if diagnostics.len() <= 1 {
            return vec![];
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    fn run_with_file_ctx(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust_with_file_ctx(source, &Check, file)
    }

    fn large_file(extra: &str) -> String {
        let mut s = "fn f() {}\n".repeat(155);
        s.push_str(extra);
        s
    }

    #[test]
    fn flags_multiple_dividers_in_large_file() {
        let src = large_file("// ============\nfn g() {}\n// ============\n");
        assert_eq!(run(&src).len(), 2);
    }

    #[test]
    fn allows_short_dashes() {
        assert!(run("// -- note\nfn f() {}").is_empty());
    }

    #[test]
    fn ignores_dividers_in_code() {
        assert!(run("fn f() { let x = \"====================\"; }").is_empty());
    }

    #[test]
    fn allows_dividers_in_test_file() {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        let src = large_file("// ============\nfn g() {}\n// ============\n");
        assert!(run_with_file_ctx(&src, &file).is_empty());
    }

    #[test]
    fn allows_dividers_in_small_file() {
        assert!(run("// ============\nfn g() {}\n// ============\n").is_empty());
    }

    #[test]
    fn allows_single_divider_in_large_file() {
        let src = large_file("// ============\nfn g() {}\n");
        assert!(run(&src).is_empty());
    }

    #[test]
    fn still_flags_multiple_dividers_in_large_file() {
        let src = large_file("// ============\nfn g() {}\n// ============\nfn h() {}\n");
        assert!(!run(&src).is_empty());
    }
}
