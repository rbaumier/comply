//! vue-scoped-styles-preferred AST backend.
//!
//! Walks `style_element` nodes and reads the raw text of their `start_tag`
//! for a `scoped` or `module` attribute. Emits a diagnostic when neither is
//! present.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::vue_template_helpers::collect_attr_names;

/// True when the `<style>` opening tag carries a `scoped` or `module`
/// attribute, either of which scopes selectors locally: Vue hashes the classes
/// of a `module` block and exposes them via `$style`, exactly as `scoped`
/// isolates a block's selectors.
///
/// Attribute names are read from the raw source between the tag name and the
/// tag terminator `>`, then tokenized with [`collect_attr_names`]. This is
/// grammar-agnostic: tree-sitter-vue does not surface a value-less boolean
/// attribute (`module`/`scoped`) that follows a valued attribute (`lang="less"`)
/// as an `attribute` node, so a per-attribute child walk misses it, whereas the
/// raw text sees every attribute regardless of ordering.
fn start_tag_has_scoping_attr(start_tag: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = start_tag.walk();
    let Some(tag_name) = start_tag
        .children(&mut cursor)
        .find(|c| c.kind() == "tag_name")
    else {
        return false;
    };
    let rest = &source[tag_name.end_byte()..];
    let end = rest.iter().position(|&b| b == b'>').unwrap_or(rest.len());
    let Ok(attrs) = std::str::from_utf8(&rest[..end]) else {
        return false;
    };
    // Drop a self-closing slash so the tokenizer only sees attribute text.
    let attrs = attrs.trim().trim_end_matches('/').trim_end();
    collect_attr_names(attrs)
        .iter()
        .any(|name| *name == "scoped" || *name == "module")
}

crate::ast_check! { on ["style_element"] => |node, source, ctx, diagnostics|
    let mut cursor = node.walk();
    let Some(start_tag) = node.children(&mut cursor).find(|c| c.kind() == "start_tag") else {
        return;
    };
    if start_tag_has_scoping_attr(start_tag, source) {
        return;
    }
    let pos = start_tag.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`<style>` without `scoped` leaks selectors globally. Add `scoped` unless global is intentional.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    #[test]
    fn flags_unscoped_style() {
        assert_eq!(run("<style>\n.x {}\n</style>").len(), 1);
    }

    #[test]
    fn allows_scoped() {
        assert!(run("<style scoped>\n.x {}\n</style>").is_empty());
    }

    #[test]
    fn allows_module() {
        assert!(run("<style module>\n.x {}\n</style>").is_empty());
    }

    #[test]
    fn allows_scoped_with_lang() {
        assert!(run("<style lang=\"scss\" scoped>\n.x {}\n</style>").is_empty());
    }

    #[test]
    fn allows_module_after_valued_lang() {
        // The exact repro: a value-less `module` after a valued `lang`.
        assert!(run("<style lang=\"less\" module>\n.a {}\n</style>").is_empty());
        assert!(run("<style lang=\"scss\" module>\n.a {}\n</style>").is_empty());
    }

    #[test]
    fn allows_scoped_after_valued_lang() {
        assert!(run("<style lang=\"less\" scoped>\n.x {}\n</style>").is_empty());
    }

    #[test]
    fn flags_unscoped_style_with_lang() {
        // A valued `lang` alone (no scoped/module) still leaks globally.
        assert_eq!(run("<style lang=\"less\">\n.x {}\n</style>").len(), 1);
    }

    #[test]
    fn flags_when_module_is_a_value_or_substring_not_a_boolean_attr() {
        // Exact attribute-name match: `module` as a *value* or inside a longer
        // name must not exempt the block.
        assert_eq!(run("<style lang=\"module\">\n.x {}\n</style>").len(), 1);
        assert_eq!(run("<style data-module>\n.x {}\n</style>").len(), 1);
    }

    #[test]
    fn allows_module_on_multiline_start_tag() {
        assert!(run("<style\n  lang=\"less\"\n  module>\n.a {}\n</style>").is_empty());
    }
}
