//! no-put-method backend — flag `method: 'PUT'` in fetch/request calls.
//!
//! Why: PUT means "replace the entire resource". Almost every partial
//! update is wrongly shipped as PUT when the author wanted PATCH. If you
//! genuinely need full replacement, you probably want a specialized
//! endpoint that takes every field explicitly. When in doubt, PATCH.
//!
//! Detection: walk `pair` nodes (object literal key:value entries) whose
//! key is `method` and value is the string literal `'PUT'` or `"PUT"`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source_bytes = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "pair" {
                return;
            }
            let Some(key) = node.child_by_field_name("key") else {
                return;
            };
            let Ok(key_text) = key.utf8_text(source_bytes) else {
                return;
            };
            // Accept "method" and 'method' and method (unquoted shorthand).
            let key_norm = key_text.trim_matches(|c| c == '"' || c == '\'');
            if key_norm != "method" {
                return;
            }
            let Some(value) = node.child_by_field_name("value") else {
                return;
            };
            let Ok(value_text) = value.utf8_text(source_bytes) else {
                return;
            };
            let value_norm = value_text.trim_matches(|c| c == '"' || c == '\'' || c == '`');
            if value_norm != "PUT" {
                return;
            }
            let pos = value.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-put-method".into(),
                message: "`method: 'PUT'` — PUT replaces the entire \
                          resource. Most update-style endpoints want PATCH \
                          (partial update). If you genuinely need full \
                          replacement, add a comment explaining why."
                    .into(),
                severity: Severity::Warning,
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
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(
            &CheckCtx::for_test(Path::new("t.ts"), source),
            &tree,
        )
    }

    #[test]
    fn flags_put_method() {
        assert_eq!(
            run_on("fetch(url, { method: 'PUT', body });").len(),
            1
        );
    }

    #[test]
    fn flags_put_method_double_quoted() {
        assert_eq!(
            run_on("fetch(url, { method: \"PUT\" });").len(),
            1
        );
    }

    #[test]
    fn allows_patch_method() {
        assert!(run_on("fetch(url, { method: 'PATCH' });").is_empty());
    }

    #[test]
    fn allows_post_get_delete() {
        for method in ["POST", "GET", "DELETE", "PATCH"] {
            let source = format!("fetch(url, {{ method: '{method}' }});");
            assert!(run_on(&source).is_empty(), "{method} should be allowed");
        }
    }
}
