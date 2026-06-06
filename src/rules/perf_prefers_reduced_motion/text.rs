//! Emit one diagnostic for the first animation node when the stylesheet
//! lacks a `prefers-reduced-motion` branch.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { prefilter = ["animation", "@keyframes"] => |node, source, ctx, diagnostics|
    if !diagnostics.is_empty() { return; }
    if ctx.source_contains("prefers-reduced-motion") { return; }

    let offender = match node.kind() {
        "keyframes_statement" => Some(node),
        "declaration" => {
            let mut c = node.walk();
            let kids: Vec<_> = node.children(&mut c).collect();
            let Some(prop) = kids.iter().find(|n| n.kind() == "property_name") else { return };
            let Ok(t) = prop.utf8_text(source) else { return };
            let lower = t.to_ascii_lowercase();
            matches!(lower.as_str(), "animation" | "animation-name").then_some(node)
        }
        _ => None,
    };
    let Some(target) = offender else { return };

    let pos = target.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "CSS declares animations/`@keyframes` but no `@media (prefers-reduced-motion: reduce)` branch — users who opt out still see motion.".into(),
        severity: Severity::Warning,
        span: Some((target.byte_range().start, target.byte_range().len())),
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
    fn flags_keyframes_without_reduced_motion() {
        let css =
            "@keyframes spin { from { transform: rotate(0); } to { transform: rotate(360deg); } }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn flags_animation_property_without_reduced_motion() {
        let css = ".spinner { animation: spin 1s linear infinite; }";
        assert_eq!(run(css).len(), 1);
    }

    #[test]
    fn allows_animation_with_reduced_motion_guard() {
        let css = r"
            .spinner { animation: spin 1s linear infinite; }
            @media (prefers-reduced-motion: reduce) {
                .spinner { animation: none; }
            }
        ";
        assert!(run(css).is_empty());
    }

    #[test]
    fn allows_css_without_animations() {
        assert!(run(".btn { color: red; }").is_empty());
    }
}
