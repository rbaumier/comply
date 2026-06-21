mod oxc_typescript;

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-no-conflicting-classes",
    description: "Mutually exclusive Tailwind classes produce unpredictable styles.",
    remediation: "Keep only the intended utility. For example, `p-4 p-6` — \
                  remove one of the two padding values.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["tailwind"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

/// Prefixes whose values are unambiguously mutually exclusive — any two
/// classes sharing one of these prefixes conflict. Ambiguous prefixes
/// (`text-`, `font-`, `flex-`, `border-`, `bg-`) are handled by dedicated
/// sub-categorisation functions that split by CSS sub-property.
pub(crate) const CONFLICT_PREFIXES: &[&str] = &[
    "p-", "px-", "py-", "pt-", "pr-", "pb-", "pl-",
    "m-", "mx-", "my-", "mt-", "mr-", "mb-", "ml-",
    "w-", "h-", "min-w-", "min-h-", "max-w-", "max-h-",
    "rounded-", "shadow-", "opacity-", "z-",
    "gap-", "gap-x-", "gap-y-", "grid-cols-", "grid-rows-", "justify-", "items-", "self-", "order-", "overflow-",
];

pub(crate) const DISPLAY_CLASSES: &[&str] = &[
    "block", "flex", "grid", "inline", "inline-block", "inline-flex",
    "inline-grid", "hidden", "table", "contents", "flow-root",
];

pub(crate) fn css_value_type(value: &str) -> Option<&'static str> {
    let v = value.trim();
    if v.starts_with('#') || v.starts_with("rgb") || v.starts_with("hsl")
        || v.starts_with("oklch") || v.starts_with("hwb") || v.starts_with("lab")
        || v.starts_with("lch") || v.starts_with("color(")
    {
        return Some("color");
    }
    const LENGTH_UNITS: &[&str] = &[
        "px", "rem", "em", "%", "vw", "vh", "dvh", "svh", "lvh", "vmin", "vmax",
        "ch", "ex", "cap", "lh", "rlh", "pt", "pc", "mm", "cm", "in",
    ];
    if LENGTH_UNITS.iter().any(|u| v.ends_with(u))
        || v.starts_with("calc(") || v.starts_with("clamp(")
        || v.starts_with("min(") || v.starts_with("max(")
    {
        return Some("length");
    }
    None
}

pub(crate) fn text_category(class: &str) -> Option<&'static str> {
    let suffix = &class[5..];

    // Non-size text-* utilities first — exact word match.
    match suffix {
        "wrap" | "nowrap" | "balance" | "pretty" => return Some("text-wrap"),
        "left" | "center" | "right" | "justify" | "start" | "end" => return Some("text-align"),
        "ellipsis" | "clip" => return Some("text-overflow"),
        "uppercase" | "lowercase" | "capitalize" | "normal-case" => return Some("text-transform"),
        "underline" | "overline" | "line-through" | "no-underline" => return Some("text-decoration"),
        _ => {}
    }

    // Font-size: strip the optional `/<line-height>` shorthand so
    // `text-base/4.5`, `text-sm/4`, `text-base/lh` all match.
    let size_part = suffix.split('/').next().unwrap_or(suffix);
    match size_part {
        // `md` is a common non-standard size alias (the "missing" step
        // between `sm` and `lg`); it is never a color name.
        "xs" | "sm" | "md" | "base" | "lg" | "xl" => return Some("text-size"),
        _ => {}
    }
    if size_part.ends_with("xl") && size_part.len() > 2 {
        return Some("text-size");
    }

    // Arbitrary value: `text-[16px]` (size) or `text-[#fff]` (color).
    if suffix.starts_with('[') && suffix.ends_with(']') {
        let inner = &suffix[1..suffix.len() - 1];
        return match css_value_type(inner) {
            Some("length") => Some("text-size"),
            Some("color") => Some("text-color"),
            _ => None,
        };
    }

    // Named color token (`text-foreground`, `text-red-500`, `text-red-500/50`).
    // `size_part` already has any `/<modifier>` (line-height or opacity) stripped.
    // Accept only shapes matching a real Tailwind text-color; a `text-*` token
    // matching none of them is not a Tailwind utility (e.g. Vuetify's
    // `text-title-large`, `text-medium-emphasis`) and must NOT bucket into the
    // conflict group.
    if is_text_color_token(size_part) {
        return Some("text-color");
    }
    None
}

/// True when `token` (the `text-` suffix, opacity already stripped) is a real
/// Tailwind/shadcn text-color value:
/// - a CSS color keyword (`inherit`, `transparent`, `black`, …);
/// - a single semantic CSS-variable color (`primary`, `muted`, custom `brand`);
/// - a palette name ending in a numeric shade (`red-500`, `neutral-50`);
/// - a shadcn `*-foreground` compound at any depth (`muted-foreground`,
///   `sidebar-primary-foreground`).
///
/// Descriptive multi-segment tokens whose final segment is neither a shade nor
/// `foreground` (Material/Vuetify typography & emphasis scales like
/// `title-large`, `medium-emphasis`) are rejected — they are not Tailwind
/// utilities and must not be grouped into the conflict bucket.
fn is_text_color_token(token: &str) -> bool {
    const COLOR_KEYWORDS: &[&str] = &["inherit", "current", "transparent", "black", "white"];
    if COLOR_KEYWORDS.contains(&token) {
        return true;
    }
    if token.is_empty() {
        return false;
    }
    let Some(last) = token.rsplit('-').next() else {
        return false;
    };
    if !token.contains('-') {
        // Single semantic CSS-variable color (`primary`, `muted`, `brand`).
        return true;
    }
    // Palette + numeric shade (`red-500`) or the shadcn `*-foreground` compound.
    (!last.is_empty() && last.chars().all(|c| c.is_ascii_digit())) || last == "foreground"
}

pub(crate) fn flex_category(class: &str) -> Option<&'static str> {
    match class {
        "flex-row" | "flex-row-reverse" | "flex-col" | "flex-col-reverse" => Some("flex-direction"),
        "flex-wrap" | "flex-wrap-reverse" | "flex-nowrap" => Some("flex-wrap"),
        "flex-1" | "flex-auto" | "flex-initial" | "flex-none" => Some("flex-shorthand"),
        _ => None,
    }
}

pub(crate) fn border_category(class: &str) -> Option<&'static str> {
    let suffix = &class[7..];
    match suffix {
        "solid" | "dashed" | "dotted" | "double" | "hidden" | "none" => return Some("border-style"),
        "collapse" | "separate" => return Some("border-collapse"),
        _ => {}
    }
    if suffix.chars().all(|c| c.is_ascii_digit()) {
        return Some("border-width");
    }
    for (side, group) in [
        ("t", "border-top"), ("r", "border-right"), ("b", "border-bottom"),
        ("l", "border-left"), ("x", "border-x"), ("y", "border-y"),
        ("s", "border-start"), ("e", "border-end"),
    ] {
        if suffix == side || suffix.starts_with(&format!("{side}-")) {
            return Some(group);
        }
    }
    Some("border-color")
}

pub(crate) fn font_category(class: &str) -> Option<&'static str> {
    match class {
        "font-sans" | "font-serif" | "font-mono" => Some("font-family"),
        "font-italic" | "font-not-italic" => Some("font-style"),
        "font-thin" | "font-extralight" | "font-light" | "font-normal" | "font-medium"
        | "font-semibold" | "font-bold" | "font-extrabold" | "font-black" => Some("font-weight"),
        _ => None,
    }
}

/// Subdivides `bg-*` utilities by the CSS sub-property they set, so utilities
/// targeting different sub-properties (`bg-cover` size + `bg-center` position +
/// `bg-no-repeat` repeat — the idiomatic full-cover-image combo) don't conflict.
/// Only two `bg-*` setting the SAME sub-property conflict. The catch-all
/// `bg-color` covers background-color/image/gradient utilities, which do conflict
/// (`bg-red-500 bg-blue-500`).
pub(crate) fn bg_category(class: &str) -> Option<&'static str> {
    // background-repeat (`bg-repeat`, `bg-no-repeat`, `bg-repeat-x/y/round/space`)
    if class == "bg-repeat" || class == "bg-no-repeat" || class.starts_with("bg-repeat-") {
        return Some("bg-repeat");
    }
    if class.starts_with("bg-clip-") {
        return Some("bg-clip"); // background-clip (e.g. `bg-clip-text` for gradient text)
    }
    if class.starts_with("bg-origin-") {
        return Some("bg-origin"); // background-origin
    }
    if class.starts_with("bg-blend-") {
        return Some("bg-blend"); // background-blend-mode
    }
    match class {
        "bg-auto" | "bg-cover" | "bg-contain" => Some("bg-size"),
        "bg-center" | "bg-top" | "bg-right" | "bg-bottom" | "bg-left"
        | "bg-left-top" | "bg-left-bottom" | "bg-right-top" | "bg-right-bottom" => {
            Some("bg-position")
        }
        "bg-fixed" | "bg-local" | "bg-scroll" => Some("bg-attachment"),
        // background-color / image / gradient — catch-all paint group.
        _ => Some("bg-color"),
    }
}

pub(crate) fn conflict_key(class: &str) -> Option<&'static str> {
    if class.starts_with("text-") { return text_category(class); }
    if class.starts_with("flex-") { return flex_category(class); }
    if class.starts_with("border-") { return border_category(class); }
    if class.starts_with("font-") { return font_category(class); }
    if class.starts_with("bg-") { return bg_category(class); }

    let mut prefixes: Vec<&&str> = CONFLICT_PREFIXES.iter().collect();
    prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for prefix in prefixes {
        if class.starts_with(*prefix) {
            return Some(prefix);
        }
    }
    if DISPLAY_CLASSES.contains(&class) {
        return Some("display");
    }
    None
}

#[cfg(test)]
mod text_category_tests {
    use super::*;

    #[test]
    fn classifies_size_shorthand_as_text_size() {
        // Regression for rbaumier/comply#13 — `text-<size>/<lh>` is a
        // font-size shorthand and must not conflict with color tokens.
        assert_eq!(text_category("text-base/4.5"), Some("text-size"));
        assert_eq!(text_category("text-sm/4"), Some("text-size"));
        assert_eq!(text_category("text-base/lh"), Some("text-size"));
        assert_eq!(text_category("text-xl/relaxed"), Some("text-size"));
    }

    #[test]
    fn color_and_size_shorthand_do_not_conflict() {
        // text-foreground (color) + text-base/4.5 (size) must produce
        // distinct conflict keys so they can coexist.
        assert_ne!(
            text_category("text-foreground"),
            text_category("text-base/4.5")
        );
    }

    #[test]
    fn plain_sizes_still_classified() {
        assert_eq!(text_category("text-xs"), Some("text-size"));
        assert_eq!(text_category("text-2xl"), Some("text-size"));
    }

    #[test]
    fn color_tokens_remain_text_color() {
        assert_eq!(text_category("text-foreground"), Some("text-color"));
        assert_eq!(text_category("text-red-500"), Some("text-color"));
    }

    #[test]
    fn md_size_alias_is_text_size_not_color() {
        // Regression for rbaumier/comply#1809 — `text-md` is a size alias,
        // not a color, so it must not share a key with `text-black`.
        assert_eq!(text_category("text-md"), Some("text-size"));
        assert_ne!(text_category("text-md"), text_category("text-black"));
    }

    #[test]
    fn vuetify_typography_and_emphasis_are_not_text_color() {
        // Regression for rbaumier/comply#4878 — `text-title-large` (Material
        // typography scale) and `text-medium-emphasis` (emphasis opacity) are
        // Vuetify utilities, not Tailwind colors. They match no known Tailwind
        // text-color shape, so they must not bucket into `text-color`.
        assert_eq!(text_category("text-title-large"), None);
        assert_eq!(text_category("text-medium-emphasis"), None);
        assert_eq!(text_category("text-body-medium"), None);
        assert_eq!(text_category("text-headline-small"), None);
    }

    #[test]
    fn real_color_tokens_still_classified() {
        // Palette shades, CSS keywords, semantic vars and the shadcn
        // `*-foreground` compound remain text-color.
        assert_eq!(text_category("text-red-500"), Some("text-color"));
        assert_eq!(text_category("text-neutral-50"), Some("text-color"));
        assert_eq!(text_category("text-black"), Some("text-color"));
        assert_eq!(text_category("text-white"), Some("text-color"));
        assert_eq!(text_category("text-transparent"), Some("text-color"));
        assert_eq!(text_category("text-primary"), Some("text-color"));
        assert_eq!(text_category("text-foreground"), Some("text-color"));
        assert_eq!(text_category("text-muted-foreground"), Some("text-color"));
        // Deep shadcn `*-foreground` compound (`text-sidebar-primary-foreground`).
        assert_eq!(
            text_category("text-sidebar-primary-foreground"),
            Some("text-color")
        );
        // Opacity modifier on a palette color (`text-red-500/50`) still color.
        assert_eq!(text_category("text-red-500/50"), Some("text-color"));
    }
}

#[cfg(test)]
mod bg_category_tests {
    use super::*;

    #[test]
    fn classifies_bg_sub_properties() {
        // Regression for rbaumier/comply#4487 — `bg-*` utilities set distinct
        // CSS sub-properties and must each get their own conflict key.
        assert_eq!(conflict_key("bg-cover"), Some("bg-size"));
        assert_eq!(conflict_key("bg-center"), Some("bg-position"));
        assert_eq!(conflict_key("bg-no-repeat"), Some("bg-repeat"));
        assert_eq!(conflict_key("bg-fixed"), Some("bg-attachment"));
        assert_eq!(conflict_key("bg-clip-text"), Some("bg-clip"));
        assert_eq!(conflict_key("bg-red-500"), Some("bg-color"));
        assert_eq!(conflict_key("bg-gradient-to-r"), Some("bg-color"));
    }

    #[test]
    fn cover_center_no_repeat_have_distinct_keys() {
        // The idiomatic full-cover-image combo must not conflict.
        assert_ne!(conflict_key("bg-cover"), conflict_key("bg-center"));
        assert_ne!(conflict_key("bg-center"), conflict_key("bg-no-repeat"));
        assert_ne!(conflict_key("bg-cover"), conflict_key("bg-no-repeat"));
    }

    #[test]
    fn same_sub_property_shares_key() {
        // Two utilities for the same sub-property still conflict.
        assert_eq!(conflict_key("bg-cover"), conflict_key("bg-contain")); // bg-size
        assert_eq!(conflict_key("bg-red-500"), conflict_key("bg-blue-500")); // bg-color
    }
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Vue,
                Backend::TreeSitter(Box::new(typescript::Check)),
            ),
        ],
    }
}
