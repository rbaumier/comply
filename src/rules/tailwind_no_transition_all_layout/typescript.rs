use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value};

const LAYOUT_PREFIXES: &[&str] = &[
    "w-", "h-", "min-w-", "min-h-", "max-w-", "max-h-", "top-", "left-", "right-", "bottom-",
    "inset-",
];

fn is_layout_utility(tok: &str) -> bool {
    let base = tok.rsplit(':').next().unwrap_or(tok);
    LAYOUT_PREFIXES.iter().any(|p| {
        let Some(rest) = base.strip_prefix(p) else { return false };
        // Animatable layout sizes: numeric scale, full/screen/auto, or arbitrary.
        !rest.is_empty()
            && (rest == "full"
                || rest == "screen"
                || rest == "auto"
                || rest.starts_with('[')
                || rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
    })
}

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let name = jsx_attribute_name(node, source).unwrap_or("");
    if name != "className" && name != "class" { return; }
    let Some(value) = jsx_attribute_string_value(node, source) else { return };

    let tokens: Vec<&str> = value.split_whitespace().collect();
    let has_transition_all = tokens.iter().any(|t| {
        let base = t.rsplit(':').next().unwrap_or(t);
        base == "transition-all" || base == "transition"
    });
    if !has_transition_all { return; }

    let has_layout = tokens.iter().any(|t| is_layout_utility(t));
    if !has_layout { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`transition-all` combined with layout utilities (w-/h-/top-/left-/…) triggers layout on every frame. Use `transition-transform` + `translate-*` or `transition-opacity` instead.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_transition_all_with_width() {
        assert_eq!(run(r#"export const A = () => <div className="transition-all w-64" />;"#).len(), 1);
    }

    #[test]
    fn flags_transition_all_with_top() {
        assert_eq!(run(r#"export const A = () => <div className="transition-all top-4" />;"#).len(), 1);
    }

    #[test]
    fn allows_transition_transform() {
        assert!(run(r#"export const A = () => <div className="transition-transform translate-x-4" />;"#).is_empty());
    }

    #[test]
    fn allows_transition_all_without_layout() {
        assert!(run(r#"export const A = () => <div className="transition-all opacity-50" />;"#).is_empty());
    }
}
