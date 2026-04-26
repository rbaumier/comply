use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_string_value};

/// Raw palette color names. These are the names hardcoded in Tailwind's
/// default theme; semantic tokens have different names (background,
/// foreground, primary, secondary, accent, muted, destructive, card,
/// popover, border, input, ring).
const RAW_COLORS: &[&str] = &[
    "white", "black", "slate", "gray", "zinc", "neutral", "stone", "red", "orange", "amber",
    "yellow", "lime", "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet",
    "purple", "fuchsia", "pink", "rose",
];

const COLOR_PREFIXES: &[&str] = &[
    "bg-", "text-", "border-", "ring-", "fill-", "stroke-", "from-", "to-", "via-", "divide-",
    "outline-", "decoration-", "placeholder-", "caret-", "accent-", "shadow-",
];

/// Return true if a single class token (already stripped of any variant
/// prefix like `hover:`) references a raw palette color.
fn is_raw_color_class(token: &str) -> bool {
    for prefix in COLOR_PREFIXES {
        let Some(rest) = token.strip_prefix(prefix) else { continue };
        // `bg-white` / `bg-black` — no numeric suffix.
        if RAW_COLORS.contains(&rest) {
            return true;
        }
        // `bg-blue-500`, `text-gray-900` — color-shade pairs.
        if let Some((color, shade)) = rest.rsplit_once('-')
            && RAW_COLORS.contains(&color) && shade.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
    }
    false
}

/// Strip Tailwind variant prefixes (`hover:`, `md:`, `dark:`, etc.) and
/// return the base utility token. `dark:bg-white` → `bg-white`.
fn strip_variants(token: &str) -> &str {
    token.rsplit(':').next().unwrap_or(token)
}

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    let name = jsx_attribute_name(node, source).unwrap_or("");
    if name != "className" && name != "class" { return; }
    let Some(value) = jsx_attribute_string_value(node, source) else { return };

    let has_raw = value
        .split_whitespace()
        .any(|tok| is_raw_color_class(strip_variants(tok)));
    if !has_raw { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Raw palette color utility in className — use semantic tokens (bg-background, text-foreground, bg-primary, …).".into(),
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
    fn flags_bg_white() {
        assert_eq!(run(r#"export const A = () => <div className="bg-white" />;"#).len(), 1);
    }

    #[test]
    fn flags_text_gray_900() {
        assert_eq!(run(r#"export const A = () => <div className="text-gray-900" />;"#).len(), 1);
    }

    #[test]
    fn flags_bg_blue_500() {
        assert_eq!(run(r#"export const A = () => <div className="p-4 bg-blue-500" />;"#).len(), 1);
    }

    #[test]
    fn allows_semantic_tokens() {
        assert!(run(r#"export const A = () => <div className="bg-background text-foreground" />;"#).is_empty());
    }

    #[test]
    fn allows_bg_primary() {
        assert!(run(r#"export const A = () => <div className="bg-primary text-primary-foreground" />;"#).is_empty());
    }
}
