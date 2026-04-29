//! tailwind-read-theme-before-classes backend — flag arbitrary Tailwind
//! values (`p-[13px]`, `bg-[#abc]`, `text-[20px]`) inside a `className` /
//! `class` string when the surrounding file never reads from the Tailwind
//! config (no `tailwind.config` import, no `resolveConfig(...)`, no
//! `theme(...)` helper). An arbitrary value is acceptable only when the
//! author has at least acknowledged the design-token surface by reading
//! from it — otherwise the arbitrary class drifts from the theme.
//!
//! Detection: walk `string_fragment` / `string` nodes, check whether the
//! text matches `<prefix>-[<value>]`, then consult the whole file source
//! once per hit for a theme-reference marker. The file-level marker check
//! is a substring scan (fast; the source is borrowed from `CheckCtx`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::jsx::{jsx_attribute_name, jsx_attribute_value};

/// Markers that indicate the file already reads the Tailwind theme/config.
/// Any single hit exempts the whole file.
const THEME_MARKERS: &[&str] = &[
    "tailwind.config",
    "tailwindConfig",
    "resolveConfig",
    "theme(",
    "from 'tailwindcss/",
    "from \"tailwindcss/",
];

/// Tailwind utility prefixes that accept arbitrary values worth flagging.
/// We only look at utilities where the arbitrary value is almost always a
/// design token (spacing, color, size, radius). Pure selectors like
/// `data-[state=open]:` or `group-[.is-open]:` are variants, not tokens,
/// so we leave them alone.
const ARBITRARY_PREFIXES: &[&str] = &[
    "p-[", "px-[", "py-[", "pt-[", "pb-[", "pl-[", "pr-[", "m-[", "mx-[", "my-[", "mt-[", "mb-[",
    "ml-[", "mr-[", "gap-[", "gap-x-[", "gap-y-[", "space-x-[", "space-y-[", "w-[", "h-[",
    "min-w-[", "min-h-[", "max-w-[", "max-h-[", "text-[", "bg-[", "border-[", "rounded-[",
    "ring-[", "shadow-[", "leading-[", "tracking-[",
];

fn class_contains_arbitrary(text: &str) -> Option<usize> {
    for prefix in ARBITRARY_PREFIXES {
        let mut search_from = 0;
        while let Some(rel) = text[search_from..].find(prefix) {
            let start = search_from + rel;
            // Word-boundary: prefix must not be glued to a preceding alnum/dash.
            if start > 0
                && text
                    .as_bytes()
                    .get(start - 1)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
            {
                search_from = start + 1;
                continue;
            }
            // Must close with `]` to be a full arbitrary-value token.
            if let Some(close) = text[start + prefix.len()..].find(']') {
                let value = &text[start + prefix.len()..start + prefix.len() + close];
                // var(--*) or bare --custom-prop (Tailwind v4) reference design tokens.
                if value.contains("var(--") || value.starts_with("--") {
                    search_from = start + prefix.len() + close + 1;
                    continue;
                }
                return Some(start);
            }
            search_from = start + prefix.len();
        }
    }
    None
}

fn file_reads_theme(source: &str) -> bool {
    THEME_MARKERS.iter().any(|m| source.contains(m))
}

crate::ast_check! { on ["string"] prefilter = ["resolveConfig"] => |node, source, ctx, diagnostics|
    // shadcn/ui primitives use arbitrary values by design.
    let path_str = ctx.path.to_str().unwrap_or("");
    if path_str.contains("/components/ui/") || path_str.contains("/lib/ui/") {
        return;
    }

    let Ok(text) = node.utf8_text(source) else { return; };
    let Some(_) = class_contains_arbitrary(text) else { return; };

    // Only flag when this string is the value of a `className` / `class`
    // attribute — otherwise we'd fire on arbitrary SQL, regex, etc.
    // Walk up to find a jsx_attribute ancestor whose name is className.
    let mut cursor = node;
    let mut in_classname = false;
    for _ in 0..6 {
        let Some(parent) = cursor.parent() else { break };
        if parent.kind() == "jsx_attribute" {
            let name = jsx_attribute_name(parent, source);
            if matches!(name, Some("className") | Some("class")) {
                // Confirm the node is inside the attribute's value subtree.
                if let Some(val) = jsx_attribute_value(parent) {
                    let ns = node.start_byte();
                    let ne = node.end_byte();
                    if ns >= val.start_byte() && ne <= val.end_byte() {
                        in_classname = true;
                    }
                }
            }
            break;
        }
        cursor = parent;
    }
    if !in_classname { return; }

    if file_reads_theme(ctx.source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "Arbitrary Tailwind value used without reading the theme. \
                  Import `tailwind.config` / call `resolveConfig(...)` / use `theme(...)`, \
                  or switch to a design-token class.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_arbitrary_padding_without_theme_read() {
        assert_eq!(run(r#"export const A = () => <div className="p-[13px]" />;"#).len(), 1);
    }

    #[test]
    fn flags_arbitrary_color_without_theme_read() {
        assert_eq!(
            run(r#"export const A = () => <div className="bg-[#ff0000]" />;"#).len(),
            1
        );
    }

    #[test]
    fn allows_arbitrary_when_file_imports_tailwind_config() {
        let src = r#"
            import tailwindConfig from '../../tailwind.config';
            export const A = () => <div className="p-[13px]" />;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arbitrary_when_file_calls_resolve_config() {
        let src = r#"
            import resolveConfig from 'tailwindcss/resolveConfig';
            const full = resolveConfig(config);
            export const A = () => <div className="p-[13px]" />;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_arbitrary_when_file_uses_theme_helper() {
        let src = r#"
            const spacing = theme('spacing.4');
            export const A = () => <div className="p-[13px]" />;
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_token_classes() {
        assert!(run(r#"export const A = () => <div className="p-4 bg-blue-500" />;"#).is_empty());
    }

    #[test]
    fn ignores_arbitrary_values_outside_classname() {
        // The arbitrary-looking text sits in a SQL string, not a className.
        let src = r#"export const q = "SELECT * FROM t WHERE p = 'p-[13px]'";"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_css_variable_in_arbitrary_value() {
        let src = r#"export const A = () => <div className="w-[var(--sidebar-width)]" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_custom_property_tailwind_v4() {
        let src = r#"export const A = () => <div className="w-[--sidebar-width]" />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_shadcn_ui_components() {
        use crate::rules::test_helpers::run_ts_with_path;
        let src = r#"export const A = <div className="ring-[3px]" />;"#;
        let d = run_ts_with_path(src, &Check, "src/components/ui/checkbox.tsx");
        assert!(d.is_empty());
    }
}
