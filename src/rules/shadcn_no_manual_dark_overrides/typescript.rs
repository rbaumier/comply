//! Flag a `className` that contains a `dark:<prefix>-<color>-<shade>`
//! variant — these are the manual overrides shadcn semantic tokens
//! obviate. `dark:bg-background` or `dark:text-primary` are allowed
//! (semantic tokens), only raw palette colors trip the rule.

use crate::diagnostic::{Diagnostic, Severity};

const COLOR_PREFIXES: &[&str] = &[
    "bg", "text", "border", "ring", "fill", "stroke", "from", "to", "via", "divide", "outline",
    "accent", "caret", "placeholder", "shadow", "decoration",
];

const COLORS: &[&str] = &[
    "slate", "gray", "zinc", "neutral", "stone", "red", "orange", "amber", "yellow", "lime",
    "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet", "purple", "fuchsia",
    "pink", "rose", "white", "black",
];

fn is_dark_raw_color(class: &str) -> bool {
    // Require a `dark:` prefix. Nested variants like `md:dark:bg-gray-900`
    // are handled by splitting on `:` and checking any segment.
    let segments: Vec<&str> = class.split(':').collect();
    if segments.len() < 2 {
        return false;
    }
    if !segments.iter().take(segments.len() - 1).any(|s| *s == "dark") {
        return false;
    }
    let utility = segments.last().copied().unwrap_or("");
    let utility = utility.trim_start_matches('!').trim_start_matches('-');
    let mut parts = utility.split('-');
    let Some(prefix) = parts.next() else {
        return false;
    };
    if !COLOR_PREFIXES.contains(&prefix) {
        return false;
    }
    // Shorthand: `dark:bg-white` / `dark:text-black` — flag those too.
    let Some(color_or_shade) = parts.next() else {
        return false;
    };
    if (color_or_shade == "white" || color_or_shade == "black") && parts.next().is_none() {
        return true;
    }
    if !COLORS.contains(&color_or_shade) {
        return false;
    }
    let Some(shade) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }
    shade.len() >= 2
        && shade.len() <= 3
        && shade.chars().all(|c| c.is_ascii_digit())
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "jsx_attribute" {
        return;
    }
    if crate::rules::jsx::jsx_attribute_name(node, source) != Some("className") {
        return;
    }
    let Some(value) = crate::rules::jsx::jsx_attribute_string_value(node, source) else {
        return;
    };
    if value.split_ascii_whitespace().any(is_dark_raw_color) {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Manual `dark:` color override — use a shadcn semantic token (e.g. `bg-background`, `text-foreground`) so theming stays DRY.".into(),
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
    fn flags_dark_bg_gray_900() {
        assert_eq!(run(r#"const x = <div className="bg-white dark:bg-gray-900">x</div>;"#).len(), 1);
    }

    #[test]
    fn flags_dark_text_white() {
        assert_eq!(run(r#"const x = <div className="text-black dark:text-white">x</div>;"#).len(), 1);
    }

    #[test]
    fn allows_dark_semantic_token() {
        assert!(run(r#"const x = <div className="dark:bg-background">x</div>;"#).is_empty());
    }

    #[test]
    fn allows_no_dark_variant() {
        assert!(run(r#"const x = <div className="bg-primary text-foreground">x</div>;"#).is_empty());
    }
}
