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

/// If `class` is a `dark:<prefix>-<color>[-<shade>]` raw-palette utility,
/// return the prefix (e.g. `bg`, `text`). Otherwise return `None`.
fn dark_raw_color_prefix(class: &str) -> Option<&'static str> {
    // Require a `dark:` segment somewhere in the chain. Nested variants
    // like `md:dark:bg-gray-900` are handled by splitting on `:` and
    // checking any segment.
    let segments: Vec<&str> = class.split(':').collect();
    if segments.len() < 2 {
        return None;
    }
    if !segments.iter().take(segments.len() - 1).any(|s| *s == "dark") {
        return None;
    }
    let utility = segments.last().copied().unwrap_or("");
    let utility = utility.trim_start_matches('!').trim_start_matches('-');
    let mut parts = utility.split('-');
    let prefix = parts.next()?;
    let matched_prefix = COLOR_PREFIXES.iter().find(|p| **p == prefix)?;
    let color_or_shade = parts.next()?;
    if (color_or_shade == "white" || color_or_shade == "black") && parts.next().is_none() {
        return Some(matched_prefix);
    }
    if !COLORS.contains(&color_or_shade) {
        return None;
    }
    let shade = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if shade.len() >= 2
        && shade.len() <= 3
        && shade.chars().all(|c| c.is_ascii_digit())
    {
        Some(matched_prefix)
    } else {
        None
    }
}

/// Does `value` contain a non-`dark:` Tailwind utility for `prefix`
/// (e.g. `bg-white`)? Used to confirm the dark variant is paired with an
/// explicit light counterpart — isolated `dark:bg-gray-900` is allowed.
fn has_light_counterpart(value: &str, prefix: &str) -> bool {
    value.split_ascii_whitespace().any(|class| {
        let segments: Vec<&str> = class.split(':').collect();
        // Skip any class that already has a `dark:` segment — we want a
        // non-dark counterpart, not the same dark utility.
        if segments.iter().take(segments.len().saturating_sub(1)).any(|s| *s == "dark") {
            return false;
        }
        let utility = segments.last().copied().unwrap_or("");
        let utility = utility.trim_start_matches('!').trim_start_matches('-');
        let class_prefix = utility.split('-').next().unwrap_or("");
        class_prefix == prefix && utility.len() > prefix.len()
    })
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
    // Only flag a `dark:<prefix>-<color>` if the same className also has a
    // non-dark `<prefix>-*` — that's the manual override pattern shadcn
    // semantic tokens replace. Isolated `dark:bg-gray-900` is left alone.
    let paired = value.split_ascii_whitespace().any(|class| {
        dark_raw_color_prefix(class).is_some_and(|prefix| has_light_counterpart(value, prefix))
    });
    if paired {
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            super::META.id,
            "Manual `dark:` color override paired with a light counterpart — use a shadcn semantic token (e.g. `bg-background`, `text-foreground`) so theming stays DRY.".into(),
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

    #[test]
    fn allows_lone_dark_variant_without_light_counterpart() {
        // No paired `bg-*` light class → dark variant is being used to add
        // a dark-mode-only color, not as a manual override of a light one.
        assert!(run(r#"const x = <div className="dark:bg-gray-900">x</div>;"#).is_empty());
    }

    #[test]
    fn allows_dark_variant_with_unrelated_light_prefix() {
        // `text-black` doesn't pair with `dark:bg-gray-900` — different prefixes.
        assert!(
            run(r#"const x = <div className="text-black dark:bg-gray-900">x</div>;"#).is_empty()
        );
    }
}
