//! vue-v-memo-requires-v-for AST backend.
//!
//! Walks `start_tag` / `self_closing_tag` nodes. For any tag that carries
//! `v-memo`, require either a sibling `v-for` directive or `v-memo="[]"`
//! (empty deps) for a static subtree.

use crate::diagnostic::{Diagnostic, Severity};

struct MemoInfo {
    has_vfor: bool,
    has_vmemo: bool,
    vmemo_value_empty: bool,
}

fn directive_value_is_empty_array(dir: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = dir.walk();
    for child in dir.children(&mut cursor) {
        if (child.kind() == "quoted_attribute_value" || child.kind() == "attribute_value")
            && let Ok(text) = child.utf8_text(source)
        {
            let trimmed = text.trim_matches(|c| c == '"' || c == '\'');
            if trimmed.trim() == "[]" {
                return true;
            }
        }
    }
    false
}

fn scan_tag(node: tree_sitter::Node, source: &[u8]) -> MemoInfo {
    let mut info = MemoInfo {
        has_vfor: false,
        has_vmemo: false,
        vmemo_value_empty: false,
    };
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "directive_attribute" {
            continue;
        }
        let mut inner = child.walk();
        let mut name_str: Option<String> = None;
        for grand in child.children(&mut inner) {
            if grand.kind() == "directive_name"
                && let Ok(name) = grand.utf8_text(source)
            {
                name_str = Some(name.to_string());
            }
        }
        if let Some(name) = name_str {
            if name == "v-for" {
                info.has_vfor = true;
            } else if name == "v-memo" {
                info.has_vmemo = true;
                info.vmemo_value_empty = directive_value_is_empty_array(child, source);
            }
        }
    }
    info
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "start_tag" && node.kind() != "self_closing_tag" {
        return;
    }
    let info = scan_tag(node, source);
    if !info.has_vmemo {
        return;
    }
    if info.has_vfor || info.vmemo_value_empty {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`v-memo` without `v-for` only makes sense as `v-memo=\"[]\"` on a static subtree.".into(),
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
    fn flags_v_memo_without_v_for() {
        assert_eq!(run("<template><div v-memo=\"[dep]\">hi</div></template>").len(), 1);
    }

    #[test]
    fn allows_v_memo_with_v_for() {
        assert!(
            run("<template><div v-for=\"x in xs\" :key=\"x.id\" v-memo=\"[x.id]\">hi</div></template>").is_empty()
        );
    }

    #[test]
    fn allows_empty_static_memo() {
        assert!(run("<template><div v-memo=\"[]\">static</div></template>").is_empty());
    }
}
