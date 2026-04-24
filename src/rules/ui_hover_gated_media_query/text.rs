//! Walk `rule_set` nodes whose selector contains `:hover` and whose block
//! has motion (`transform` / `scale(` / `translate(`). Flag when no ancestor
//! `media_statement` has a `hover: hover` feature query.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "rule_set" { return; }

    let mut c = node.walk();
    let Some(selectors) = node.children(&mut c).find(|n| n.kind() == "selectors") else { return };
    let Ok(sel_text) = selectors.utf8_text(source) else { return };
    if !sel_text.contains(":hover") { return; }

    let mut bc = node.walk();
    let Some(block) = node.children(&mut bc).find(|n| n.kind() == "block") else { return };
    if !block_has_motion(block, source) { return; }

    // Walk up ancestors — if we find a `@media (hover: hover)` gate, skip.
    let mut p = node.parent();
    while let Some(parent) = p {
        if parent.kind() == "media_statement" && media_gates_hover(parent, source) {
            return;
        }
        p = parent.parent();
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`:hover` with motion (`{}`) is not gated by `@media (hover: hover) and (pointer: fine)` — touch devices will stick in the hovered state.",
            sel_text.trim()
        ),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

fn block_has_motion(block: tree_sitter::Node, source: &[u8]) -> bool {
    let mut c = block.walk();
    for decl in block.children(&mut c) {
        if decl.kind() != "declaration" { continue; }
        let Ok(txt) = decl.utf8_text(source) else { continue };
        let lower = txt.to_ascii_lowercase();
        let has_transform_prop = lower.trim_start().starts_with("transform:");
        let has_fn = lower.contains("scale(") || lower.contains("translate(");
        if has_transform_prop || has_fn {
            return true;
        }
    }
    false
}

fn media_gates_hover(media: tree_sitter::Node, source: &[u8]) -> bool {
    // Look for any descendant feature_query containing `hover: hover`.
    let mut stack: Vec<tree_sitter::Node> = vec![media];
    while let Some(n) = stack.pop() {
        // Don't descend into the media body.
        if n.kind() == "block" { continue; }
        if n.kind() == "feature_query"
            && let Ok(t) = n.utf8_text(source)
                && t.to_ascii_lowercase().replace(' ', "").contains("hover:hover") {
                    return true;
                }
        let mut c = n.walk();
        for child in n.children(&mut c) {
            stack.push(child);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_css(source, &Check)
    }

    #[test]
    fn flags_hover_transform_without_gate() {
        assert_eq!(run(".btn:hover { transform: scale(1.05); }").len(), 1);
    }

    #[test]
    fn allows_hover_in_media_query() {
        let css = r"
            @media (hover: hover) and (pointer: fine) {
                .btn:hover { transform: scale(1.05); }
            }
        ";
        assert!(run(css).is_empty());
    }

    #[test]
    fn allows_hover_without_motion() {
        assert!(run(".btn:hover { background: blue; }").is_empty());
    }
}
