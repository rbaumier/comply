//! Walk keyframe declarations and flag properties that animate layout/paint
//! instead of sticking to `transform` / `opacity`.

use crate::diagnostic::{Diagnostic, Severity};

const FORBIDDEN: &[&str] = &[
    "top", "left", "right", "bottom",
    "width", "height",
    "margin", "margin-top", "margin-bottom", "margin-left", "margin-right",
    "padding", "padding-top", "padding-bottom", "padding-left", "padding-right",
];

crate::ast_check! { on ["declaration"] => |node, source, ctx, diagnostics|
    // Only fire inside a `@keyframes { ... }`.
    let mut p = node.parent();
    let mut inside_keyframes = false;
    while let Some(parent) = p {
        if parent.kind() == "keyframes_statement" {
            inside_keyframes = true;
            break;
        }
        p = parent.parent();
    }
    if !inside_keyframes { return; }

    let mut cursor = node.walk();
    let Some(prop_node) = node.children(&mut cursor).find(|c| c.kind() == "property_name") else { return };
    let Ok(prop) = prop_node.utf8_text(source) else { return };
    let lower = prop.to_ascii_lowercase();
    if !FORBIDDEN.iter().any(|p| *p == lower) { return; }

    let pos = prop_node.start_position();
    let range = prop_node.byte_range();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`@keyframes` animates `{lower}` — prefer animating `transform`/`opacity` to stay off the layout/paint path."
        ),
        severity: Severity::Warning,
        span: Some((range.start, range.len())),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_css;

    fn run(source: &str) -> Vec<Diagnostic> {
        run_css(source, &Check)
    }

    #[test]
    fn flags_top_in_keyframes() {
        let css = "@keyframes slide { from { top: 0; } to { top: 100px; } }";
        assert!(!run(css).is_empty());
    }

    #[test]
    fn flags_width_in_keyframes() {
        let css = "@keyframes grow { from { width: 0; } to { width: 100px; } }";
        assert!(!run(css).is_empty());
    }

    #[test]
    fn allows_transform_and_opacity() {
        let css = "@keyframes fade { from { opacity: 0; transform: translateY(-10px); } to { opacity: 1; transform: none; } }";
        assert!(run(css).is_empty());
    }

    #[test]
    fn allows_top_outside_keyframes() {
        assert!(run(".hero { top: 0; }").is_empty());
    }
}
