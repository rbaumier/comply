use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value};

/// Base (variant-stripped) utility that starts an animation or transition.
fn is_animation_base(base: &str) -> bool {
    (base == "transition" || base.starts_with("transition-"))
        && base != "transition-none"
        || base == "animate-spin"
        || base == "animate-ping"
        || base == "animate-pulse"
        || base == "animate-bounce"
        || (base.starts_with("animate-") && base != "animate-none")
}

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let name = jsx_attribute_name(node, source).unwrap_or("");
    if name != "className" && name != "class" { return; }
    let Some(value) = jsx_attribute_string_value(node, source) else { return };

    let mut has_motion = false;
    let mut has_reduce = false;

    for tok in value.split_whitespace() {
        if tok.starts_with("motion-reduce:") {
            has_reduce = true;
            continue;
        }
        if tok.starts_with("motion-safe:") {
            // `motion-safe:animate-*` means the animation is already opt-in,
            // which is also acceptable for the `prefers-reduced-motion` audience.
            has_reduce = true;
            continue;
        }
        let base = tok.rsplit(':').next().unwrap_or(tok);
        if is_animation_base(base) {
            has_motion = true;
        }
    }

    if has_motion && !has_reduce {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Animation/transition without `motion-reduce:*` — users with `prefers-reduced-motion: reduce` will still see the animation.".into(),
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_transition_without_motion_reduce() {
        assert_eq!(run(r#"export const A = () => <div className="transition-colors duration-300" />;"#).len(), 1);
    }

    #[test]
    fn flags_animate_spin_without_motion_reduce() {
        assert_eq!(run(r#"export const A = () => <div className="animate-spin" />;"#).len(), 1);
    }

    #[test]
    fn allows_motion_reduce_pair() {
        assert!(run(r#"export const A = () => <div className="transition-colors motion-reduce:transition-none" />;"#).is_empty());
    }

    #[test]
    fn allows_static_classes() {
        assert!(run(r#"export const A = () => <div className="p-4 bg-card" />;"#).is_empty());
    }
}
