//! rust-no-errors-doc-section backend.
//!
//! Scans every `line_comment`/`block_comment` node for a rustdoc `# Errors`
//! markdown heading, per
//! [`doc_section_heading_span`][crate::rules::rust_helpers::doc_section_heading_span],
//! and anchors the diagnostic on the heading line rather than on the whole
//! comment. Only doc comments count, so a plain `// errors` note is untouched,
//! and only a heading counts, so prose mentioning errors is untouched.
//!
//! tree-sitter-rust makes each `///` line its own `line_comment` node, so a
//! `///`-style section is reported once, on its heading line; a `/** */` block
//! is one node whose heading line is located inside the block's text.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::doc_section_heading_span;
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, &["line_comment", "block_comment"]) {
            let Ok(text) = node.utf8_text(source) else {
                continue;
            };
            let Some((offset, length)) = doc_section_heading_span(text, "Errors") else {
                continue;
            };
            diagnostics.push(Diagnostic::at_offset(
                ctx.path,
                ctx.source,
                (node.start_byte() + offset, length),
                super::META.id,
                "`# Errors` restates the signature. The `Result` says the call can \
                 fail and the error type says how — delete the section."
                    .into(),
                Severity::Error,
            ));
        }
        diagnostics
    }
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
    fn flags_the_errors_section_of_a_line_doc_comment() {
        let src = "\
/// Build the provider adapters, run the call, then count and stamp its end.
///
/// # Errors
///
/// [`CoreError`] when a provider cannot be reached.
pub fn drive() -> Result<(), CoreError> { Ok(()) }
";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn flags_the_errors_section_of_a_block_doc_comment() {
        let src = "\
/** Drive the call.
 *
 * # Errors
 *
 * [`CoreError`] when a provider cannot be reached.
 */
pub fn drive() -> Result<(), CoreError> { Ok(()) }
";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
    }

    #[test]
    fn flags_an_errors_section_in_module_docs() {
        let src = "//! Provider driver.\n//!\n//! # Errors\n//!\n//! Anything.\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_the_panics_and_safety_sections() {
        let src = "\
/// Drive the call.
///
/// # Panics
///
/// When the registry is empty.
///
/// # Safety
///
/// The pointer must outlive the call.
pub unsafe fn drive() {}
";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_prose_mentioning_errors() {
        let src = "/// Collects the errors the drivers reported.\nfn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_plain_comment_shaped_like_a_heading() {
        let src = "// # Errors\nfn f() {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_a_heading_that_merely_starts_with_errors() {
        let src = "/// # Errors and retries\n///\n/// Prose.\nfn f() {}";
        assert!(run(src).is_empty());
    }
}
