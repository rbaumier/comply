//! vue-no-unescaped-entities — Vue AST backend.
//!
//! Walks `text` nodes inside the Vue template. These nodes contain only
//! raw text content — `{{ }}` interpolations are separate `interpolation`
//! nodes, so they are never inspected.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::collect_nodes_of_kinds;

#[derive(Debug)]
pub struct Check;

const PROBLEMATIC: &[char] = &['"', '\''];

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in collect_nodes_of_kinds(tree, &["text"]) {
            let Ok(text) = node.utf8_text(ctx.source.as_bytes()) else {
                continue;
            };
            if !text.contains(PROBLEMATIC) {
                continue;
            }
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "vue-no-unescaped-entities".into(),
                message: "Unescaped entity in template text — use the HTML entity instead."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parse");
        let path = std::path::PathBuf::from("component.vue");
        Check.check(&CheckCtx::for_test(&path, source), &tree)
    }

    #[test]
    fn flags_unescaped_quote() {
        let src = "<template>\n  <div>She said \"hello\"</div>\n</template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_clean_text() {
        let src = "<template>\n  <div>Hello world</div>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mustache_interpolation() {
        let src = "<template>\n  <h1>{{ t('home.welcome') }}</h1>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_mustache_with_closing_braces() {
        let src = "<template>\n  <span>{{ items.length }}</span>\n</template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_quote_outside_mustache() {
        let src = "<template>\n  <div>He said \"hi\" {{ name }}</div>\n</template>";
        assert_eq!(run(src).len(), 1);
    }
}
