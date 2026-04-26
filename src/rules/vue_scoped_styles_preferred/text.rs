//! vue-scoped-styles-preferred AST backend.
//!
//! Walks `style_element` nodes and inspects their `start_tag` for the
//! `scoped` or `module` attribute. Emits a diagnostic when neither is
//! present.

use crate::diagnostic::{Diagnostic, Severity};

fn start_tag_has_attr(start_tag: tree_sitter::Node, source: &[u8], target: &str) -> bool {
    let mut cursor = start_tag.walk();
    for child in start_tag.children(&mut cursor) {
        if child.kind() != "attribute" {
            continue;
        }
        let mut inner = child.walk();
        for grand in child.children(&mut inner) {
            if grand.kind() == "attribute_name"
                && let Ok(name) = grand.utf8_text(source)
                && name == target
            {
                return true;
            }
        }
    }
    false
}

crate::ast_check! { on ["style_element"] => |node, source, ctx, diagnostics|
    let mut cursor = node.walk();
    let Some(start_tag) = node.children(&mut cursor).find(|c| c.kind() == "start_tag") else {
        return;
    };
    if start_tag_has_attr(start_tag, source, "scoped")
        || start_tag_has_attr(start_tag, source, "module")
    {
        return;
    }
    let pos = start_tag.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`<style>` without `scoped` leaks selectors globally. Add `scoped` unless global is intentional.".into(),
        severity: Severity::Warning,
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
}
