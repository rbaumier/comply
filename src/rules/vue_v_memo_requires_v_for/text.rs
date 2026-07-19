//! vue-v-memo-requires-v-for AST backend.
//!
//! Walks `start_tag` / `self_closing_tag` nodes. A `v-memo` with a non-empty
//! dependency array is a valid standalone subtree memoization (documented Vue
//! behaviour, with or without `v-for`) and is never flagged. Only `v-memo="[]"`
//! without a sibling `v-for` is flagged: an empty array never re-renders, which
//! `v-once` expresses directly. `v-memo="[]"` on a `v-for` element is left
//! alone (it force-freezes the rendered rows).

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

crate::ast_check! { on ["start_tag", "self_closing_tag"] prefilter = ["v-memo"] => |node, source, ctx, diagnostics|    let info = scan_tag(node, source);
    if !info.has_vmemo {
        return;
    }
    if info.has_vfor || !info.vmemo_value_empty {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`v-memo=\"[]\"` without `v-for` never re-renders; use `v-once` instead.".into(),
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
    fn allows_standalone_v_memo_with_deps() {
        // Documented Vue pattern: subtree memoization based on arbitrary
        // conditions, no `v-for` required. See issue #4923.
        assert!(run("<template><div v-memo=\"[dep]\">hi</div></template>").is_empty());
    }

    #[test]
    fn allows_standalone_v_memo_with_multiple_deps() {
        // radix-vue ListboxItem.vue:75 — the exact false positive from #4923.
        assert!(
            run("<template><Primitive v-memo=\"[isHighlighted, isSelected, disabled, rootContext.focusable.value]\" role=\"option\" /></template>").is_empty()
        );
    }

    #[test]
    fn allows_v_memo_with_v_for() {
        assert!(
            run("<template><div v-for=\"x in xs\" :key=\"x.id\" v-memo=\"[x.id]\">hi</div></template>").is_empty()
        );
    }

    #[test]
    fn allows_empty_memo_on_v_for() {
        // `v-memo="[]"` on a `v-for` element force-freezes the rendered rows.
        assert!(
            run("<template><div v-for=\"x in xs\" :key=\"x.id\" v-memo=\"[]\">hi</div></template>").is_empty()
        );
    }

    #[test]
    fn flags_empty_standalone_memo() {
        // `v-memo="[]"` without `v-for` never re-renders — redundant with `v-once`.
        assert_eq!(
            run("<template><div v-memo=\"[]\">static</div></template>").len(),
            1
        );
    }

    #[test]
    fn ignores_non_array_standalone_memo() {
        // Only a literal empty array is the `v-once` smell; any other value is
        // treated as a real dependency and left alone (never a false positive).
        assert!(run("<template><div v-memo=\"foo\">hi</div></template>").is_empty());
    }
}
