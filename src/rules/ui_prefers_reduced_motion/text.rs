//! Emit a single diagnostic for the first animation/transition node when the
//! stylesheet lacks a `prefers-reduced-motion` branch.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["keyframes_statement", "declaration"] => |node, source, ctx, diagnostics|
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
            matches!(lower.as_str(), "animation" | "animation-name" | "transition" | "transition-property")
                .then_some(node)
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
        message: "CSS declares motion (animation/transition) without a `@media (prefers-reduced-motion: reduce)` branch.".into(),
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
    fn flags_transition_without_guard() {
        assert_eq!(run(".b { transition: opacity 0.3s; }").len(), 1);
    }

    #[test]
    fn flags_animation_without_guard() {
        assert_eq!(run(".s { animation: spin 1s linear infinite; }").len(), 1);
    }

    #[test]
    fn allows_motion_with_guard() {
        let css = r"
            .s { animation: spin 1s linear infinite; }
            @media (prefers-reduced-motion: reduce) { .s { animation: none; } }
        ";
        assert!(run(css).is_empty());
    }

    #[test]
    fn allows_no_motion() {
        assert!(run(".b { color: red; }").is_empty());
    }
}
