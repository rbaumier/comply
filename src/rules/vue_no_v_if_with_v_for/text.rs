//! vue-no-v-if-with-v-for AST backend.
//!
//! Walks `start_tag` / `self_closing_tag` nodes and reports when a tag has
//! both a `v-for` and a `v-if` directive attribute.

use crate::diagnostic::{Diagnostic, Severity};

fn tag_has_both_directives(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut has_vfor = false;
    let mut has_vif = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "directive_attribute" {
            continue;
        }
        let mut inner = child.walk();
        for grand in child.children(&mut inner) {
            if grand.kind() == "directive_name"
                && let Ok(name) = grand.utf8_text(source)
            {
                if name == "v-for" {
                    has_vfor = true;
                } else if name == "v-if" {
                    has_vif = true;
                }
            }
        }
    }
    has_vfor && has_vif
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "start_tag" && node.kind() != "self_closing_tag" {
        return;
    }
    if !tag_has_both_directives(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`v-if` on the same element as `v-for` is an anti-pattern. Wrap the `v-for` in a `<template v-if>` or filter in a computed.".into(),
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
    fn flags_both_on_same_element() {
        assert_eq!(
            run("<template><li v-for=\"x in xs\" v-if=\"x.ok\">{{ x }}</li></template>").len(),
            1
        );
    }

    #[test]
    fn allows_v_for_alone() {
        assert!(run("<template><li v-for=\"x in xs\">{{ x }}</li></template>").is_empty());
    }

    #[test]
    fn allows_v_if_alone() {
        assert!(run("<template><li v-if=\"show\">hi</li></template>").is_empty());
    }
}
