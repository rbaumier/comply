use crate::diagnostic::{Diagnostic, Severity};

fn collect_keyframe_blocks<'t>(node: tree_sitter::Node<'t>) -> Vec<tree_sitter::Node<'t>> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return out;
    }
    loop {
        let n = cursor.node();
        if n.kind() == "keyframe_block_list" {
            let mut bc = n.walk();
            for kb in n.children(&mut bc) {
                if kb.kind() == "keyframe_block" {
                    out.push(kb);
                }
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    out
}

fn keyframe_selector_text<'t>(kb: tree_sitter::Node<'t>, source: &[u8]) -> String {
    // Take everything before the block as the selector.
    let mut c = kb.walk();
    let mut parts = Vec::new();
    for ch in kb.children(&mut c) {
        if ch.kind() == "block" {
            break;
        }
        let t = ch.utf8_text(source).unwrap_or("").trim();
        if !t.is_empty() {
            parts.push(t.to_ascii_lowercase());
        }
    }
    parts.join(" ")
}

crate::ast_check! { on ["keyframes_statement"] => |node, source, ctx, diagnostics|
    let blocks = collect_keyframe_blocks(node);
    let mut seen: Vec<String> = Vec::new();
    for kb in blocks {
        let sel = keyframe_selector_text(kb, source);
        if sel.is_empty() { continue; }
        if seen.iter().any(|s| s == &sel) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &kb,
                super::META.id,
                format!("Duplicate keyframe selector `{sel}`."),
                Severity::Warning,
            ));
        } else {
            seen.push(sel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_css(s, &Check)
    }

    #[test]
    fn flags_duplicate_from() {
        let css = "@keyframes fade { from { opacity: 0; } from { opacity: 0.5; } }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_distinct_selectors() {
        let css = "@keyframes fade { from { opacity: 0; } to { opacity: 1; } }";
        assert!(run(css).is_empty());
    }
}
