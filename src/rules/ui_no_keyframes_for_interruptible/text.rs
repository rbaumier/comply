//! Flag class-driven `rule_set` nodes that invoke a `@keyframes` animation.
//! Class-toggled states cannot interrupt a keyframe animation — they should
//! interpolate via `transition` so the in-flight value is respected.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["rule_set"] => |node, source, ctx, diagnostics|
    // Selector must start with `.` (class-driven state).
    let mut c = node.walk();
    let Some(selectors) = node.children(&mut c).find(|n| n.kind() == "selectors") else { return };
    let Ok(sel) = selectors.utf8_text(source) else { return };
    let sel = sel.trim();
    if !sel.starts_with('.') { return; }

    // Body must declare an animation whose name matches a known @keyframes.
    let mut bc = node.walk();
    let Some(block) = node.children(&mut bc).find(|n| n.kind() == "block") else { return };
    let Some(anim_name) = animation_name(block, source) else { return };

    // Scan the stylesheet for a keyframes_statement with matching keyframes_name.
    let root = {
        let mut r = node;
        while let Some(p) = r.parent() { r = p; }
        r
    };
    if !stylesheet_has_keyframes(root, source, &anim_name) { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Class selector `{sel}` runs a `@keyframes` animation — swap for `transition` so toggles can interrupt."
        ),
        severity: Severity::Warning,
        span: Some((node.byte_range().start, node.byte_range().len())),
    });
}

/// First token of the `animation:` / `animation-name:` declaration's value,
/// which is the keyframes identifier in the CSS `animation` shorthand.
fn animation_name(block: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut c = block.walk();
    for decl in block.children(&mut c) {
        if decl.kind() != "declaration" { continue; }
        let mut dc = decl.walk();
        let kids: Vec<_> = decl.children(&mut dc).collect();
        let prop = kids.iter().find(|n| n.kind() == "property_name")?;
        let Ok(prop_text) = prop.utf8_text(source) else { continue };
        let prop_lower = prop_text.to_ascii_lowercase();
        if prop_lower != "animation" && prop_lower != "animation-name" { continue; }
        // First plain_value after the property is the name in the shorthand.
        for n in kids.iter().skip_while(|n| n.kind() != "property_name").skip(1) {
            if n.kind() == "plain_value"
                && let Ok(t) = n.utf8_text(source) {
                    return Some(t.trim().to_string());
                }
        }
    }
    None
}

fn stylesheet_has_keyframes(root: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    let mut stack: Vec<tree_sitter::Node> = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "keyframes_statement" {
            let mut c = n.walk();
            if n.children(&mut c).any(|child| {
                child.kind() == "keyframes_name"
                    && child.utf8_text(source).is_ok_and(|t| t.trim() == name)
            }) {
                return true;
            }
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
    fn flags_class_running_keyframes() {
        let css = r"
            @keyframes fade { from { opacity: 0; } to { opacity: 1; } }
            .is-open { animation: fade 0.3s; }
        ";
        assert!(!run(css).is_empty());
    }

    #[test]
    fn allows_transition_based_class() {
        let css = r"
            .is-open { transition: opacity 0.3s; opacity: 1; }
        ";
        assert!(run(css).is_empty());
    }

    #[test]
    fn ignores_keyframes_without_class_usage() {
        let css = r"
            @keyframes spin { from { transform: rotate(0); } to { transform: rotate(360deg); } }
        ";
        assert!(run(css).is_empty());
    }
}
