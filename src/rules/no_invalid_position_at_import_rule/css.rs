//! Port of Biome `noInvalidPositionAtImportRule`.
//!
//! Every `@import` must precede all other at-rules and style rules in a
//! stylesheet, with two exceptions that are transparent to the position check:
//! `@charset` and `@layer` (in either the `@layer name;` statement form or the
//! `@layer name { }` block form). An `@import` is flagged when any other item —
//! a style rule or any non-`@layer`/non-`@charset` at-rule — appears before it.
//!
//! tree-sitter-css is case-sensitive on at-rule keywords, so a mixed-case
//! `@imPort` parses as an error node and is invisible to this check.

use crate::diagnostic::{Diagnostic, Severity};

/// Whether `node` is an `@layer` at-rule (statement or block form). tree-sitter
/// surfaces both forms as a generic `at_rule` whose `at_keyword` child reads
/// `@layer`; the keyword is matched case-insensitively. Both forms are neutral
/// for the position check, mirroring Biome's `as_css_layer_at_rule` skip.
fn is_layer_at_rule(node: &tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "at_rule" {
        return false;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == "at_keyword")
        .and_then(|kw| kw.utf8_text(source).ok())
        .is_some_and(|kw| kw.eq_ignore_ascii_case("@layer"))
}

crate::ast_check! { on ["stylesheet"] => |node, source, ctx, diagnostics|
    // Walk top-level items in source order, tracking whether a "blocking" item —
    // anything other than @charset, @layer, an @import, or a comment — has
    // appeared. Any @import seen after a blocking item is in an invalid position.
    let mut blocking_seen = false;
    let mut cursor = node.walk();
    for item in node.named_children(&mut cursor) {
        match item.kind() {
            // Comments are trivia, not stylesheet items in Biome's model.
            "comment" => continue,
            // @charset is always permitted before @import.
            "charset_statement" => continue,
            "import_statement" => {
                if blocking_seen {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &item,
                        super::META.id,
                        "This @import is in the wrong position. Any @import must precede all \
                         other at-rules and style rules (ignoring @charset and @layer)."
                            .into(),
                        Severity::Error,
                    ));
                }
            }
            _ => {
                // @layer (statement or block) is transparent; everything else —
                // other at-rules and style rules — blocks subsequent imports.
                if !is_layer_at_rule(&item, source) {
                    blocking_seen = true;
                }
            }
        }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.css")
    }

    // --- Biome valid fixtures: must not fire. ---

    #[test]
    fn valid_import_before_rule() {
        // valid.css
        assert!(run("@import 'foo.css';\na {}").is_empty());
    }

    #[test]
    fn valid_multiple_imports() {
        // validMultipleImport.css
        assert!(run("@import 'foo.css';\n@import 'bar.css';\na {}").is_empty());
    }

    #[test]
    fn valid_charset_before_import() {
        // validCharset.css
        assert!(run("@charset \"utf-8\";\n@import 'foo.css';").is_empty());
    }

    #[test]
    fn valid_charset_multiple_import() {
        // validCharsetMultipleImport.css
        assert!(
            run("@charset \"utf-8\";\n@import 'foo.css';\n@import 'bar.css';")
                .is_empty()
        );
    }

    #[test]
    fn valid_comment_before_import() {
        // validComment.css
        assert!(run("/* some comment */\n@import 'foo.css';").is_empty());
    }

    #[test]
    fn valid_charset_comment_before_import() {
        // validCharsetComment.css
        assert!(
            run("@charset \"utf-8\";\n/* some comment */\n@import 'foo.css';").is_empty()
        );
    }

    #[test]
    fn valid_layer_statement_before_import() {
        // validLayerImport.css — `@layer name;` statement before @import.
        assert!(run("@layer default;\n@import url(theme.css) layer(theme);").is_empty());
    }

    #[test]
    fn valid_layer_block_before_import() {
        // A `@layer name { }` block is also transparent, matching Biome's
        // `as_css_layer_at_rule` skip (block-vs-statement makes no difference).
        assert!(run("@layer foo { a {} }\n@import 'x.css';").is_empty());
    }

    #[test]
    fn valid_no_important() {
        // validNoImportant.css — @import is first, a rule follows.
        assert!(run("@import 'foo.css';\na { color: red; }").is_empty());
    }

    // --- Biome invalid fixtures: must fire. ---

    #[test]
    fn invalid_import_after_rule() {
        // invalid.css — `a {}` then two imports → both imports flagged.
        let diags = run("a {}\n@import 'foo.css';\n@import 'bar.css';");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn invalid_between_imports() {
        // invalidBetweenImport.css — a style rule splits the import block;
        // every @import after the first rule is flagged.
        let css = "@import 'foo.css';\n\
                   a {}\n\
                   @import 'bar1.css';\n\
                   @import 'bar2.css';\n\
                   a {}\n\
                   @import 'bar3.css';\n\
                   @import 'bar4.css';";
        assert_eq!(run(css).len(), 4);
    }

    #[test]
    fn invalid_media_then_import() {
        // invalidMediaImport.css — @media block before @import.
        let diags = run("@media print {}\n@import url('foo.css');");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn mixed_case_import_is_invisible_to_tree_sitter() {
        // invalidMediaImportUpperCase.css uses `@imPort`. tree-sitter-css is
        // case-sensitive on at-rule keywords, so `@imPort`/`@IMPORT` parse as an
        // ERROR node rather than an `import_statement`; there is no import node to
        // flag. Biome's own case-insensitive parser does report it.
        assert!(run("@media print {}\n@imPort URl('foo.css');").is_empty());
    }

    // --- Block-vs-statement @layer: a non-@layer at-rule still blocks. ---

    #[test]
    fn non_layer_at_rule_blocks_import() {
        // @font-face is neither @charset nor @layer → it blocks a later @import.
        let diags = run("@font-face { font-family: x; }\n@import 'x.css';");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn layer_block_does_not_block_but_following_rule_does() {
        // @layer block is transparent, but the trailing rule_set blocks the
        // second import.
        let diags = run("@layer foo { a {} }\n@import 'a.css';\nb {}\n@import 'c.css';");
        assert_eq!(diags.len(), 1);
    }
}
