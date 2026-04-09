//! no-dangerously-set-inner-html backend — flag React's
//! `dangerouslySetInnerHTML` prop.
//!
//! Why: the API is called "dangerously" for a reason. Any user-controlled
//! HTML passed through it becomes an XSS vector. If you genuinely need to
//! render HTML (from a CMS, markdown, etc.), sanitize via DOMPurify first
//! and add a code comment explaining the provenance — but the default
//! answer is "don't".

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "jsx_attribute" {
                return;
            }
            let Some(name_node) = node.child(0) else {
                return;
            };
            let Ok(name) = name_node.utf8_text(source_bytes) else {
                return;
            };
            if name != "dangerouslySetInnerHTML" {
                return;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-dangerously-set-inner-html".into(),
                message: "`dangerouslySetInnerHTML` is an XSS vector. If you \
                          must render user-facing HTML, sanitize it with \
                          DOMPurify first and add a comment explaining the \
                          content's provenance."
                    .into(),
                severity: Severity::Error,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TSX.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx {
                path: Path::new("t.tsx"),
                source,
            },
            &tree,
        )
    }

    #[test]
    fn flags_dangerously_set_inner_html() {
        let source =
            "const x = <div dangerouslySetInnerHTML={{ __html: raw }} />;";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_regular_jsx_attributes() {
        assert!(run_on("const x = <div className='foo'>text</div>;").is_empty());
    }
}
