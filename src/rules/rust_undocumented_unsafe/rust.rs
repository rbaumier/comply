//! rust-undocumented-unsafe backend.
//!
//! Flags `unsafe { ... }` blocks that are not preceded by a
//! `// SAFETY: ...` comment explaining the invariants being upheld.
//! Every `unsafe` block is a promise the author makes to the compiler;
//! a code comment is how that promise is documented for reviewers and
//! for future debugging when memory corruption shows up.
//!
//! This rule is equivalent to `clippy::undocumented_unsafe_blocks`,
//! which is in the restriction group and off by default. Running it
//! via comply means consuming crates don't have to opt in — every
//! `unsafe` block in the project must carry its safety justification.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let _ = source_bytes;
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            // tree-sitter-rust kind for `unsafe { ... }` expressions.
            if node.kind() != "unsafe_block" {
                return;
            }
            if has_safety_comment_above(node, ctx.source) {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "rust-undocumented-unsafe".into(),
                message: "`unsafe` block without a `// SAFETY:` comment. \
                          Explain which invariants you're upholding — \
                          future debuggers (including you) will need \
                          that justification when memory corruption hits."
                    .into(),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

/// True if the line directly above the unsafe block contains a
/// `// SAFETY:` comment. We scan by text (the comment may be on any
/// of the preceding lines up to the previous non-blank code line)
/// because tree-sitter doesn't attach comments to expressions.
fn has_safety_comment_above(node: tree_sitter::Node, source: &str) -> bool {
    let start_row = node.start_position().row;
    if start_row == 0 {
        return false;
    }
    let lines: Vec<&str> = source.lines().collect();
    // Walk upward past blank lines / other comments until we hit code.
    let mut row = start_row;
    while row > 0 {
        row -= 1;
        let Some(line) = lines.get(row) else { break };
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            if trimmed.contains("SAFETY:") || trimmed.contains("Safety:") {
                return true;
            }
            continue;
        }
        // Hit real code — stop looking.
        break;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_bare_unsafe_block() {
        let source = "fn f(p: *const u8) { unsafe { let _ = *p; } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_unsafe_with_safety_comment() {
        let source = "fn f(p: *const u8) {\n\
                      // SAFETY: p is non-null and points to valid memory.\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unsafe_with_multi_line_comment() {
        let source = "fn f(p: *const u8) {\n\
                      // SAFETY: caller guarantees non-null.\n\
                      //         See the docs on this function.\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_unsafe_fn_declaration() {
        // `unsafe fn foo()` is not a block — we only care about `unsafe { }`.
        assert!(run_on("unsafe fn f() {}").is_empty());
    }
}
