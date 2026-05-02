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
};

/// Prefixes whose values are unambiguously mutually exclusive.
pub(crate) const CONFLICT_PREFIXES: &[&str] = &[
    "p-", "px-", "py-", "pt-", "pr-", "pb-", "pl-",
    "m-", "mx-", "my-", "mt-", "mr-", "mb-", "ml-",
    "w-", "h-", "min-w-", "min-h-", "max-w-", "max-h-",
    "bg-", "rounded-", "shadow-", "opacity-", "z-",
    "gap-", "grid-cols-", "grid-rows-", "justify-", "items-", "self-", "order-", "overflow-",
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
    match suffix {
        "xs" | "sm" | "base" | "lg" | "xl" => return Some("text-size"),
        "wrap" | "nowrap" | "balance" | "pretty" => return Some("text-wrap"),
        "left" | "center" | "right" | "justify" | "start" | "end" => return Some("text-align"),
        "ellipsis" | "clip" => return Some("text-overflow"),
        "uppercase" | "lowercase" | "capitalize" | "normal-case" => return Some("text-transform"),
        "underline" | "overline" | "line-through" | "no-underline" => return Some("text-decoration"),
        _ => {}
    }
    if suffix.ends_with("xl") && suffix.len() > 2 {
        return Some("text-size");
    }
    if suffix.starts_with('[') && suffix.ends_with(']') {
        let inner = &suffix[1..suffix.len() - 1];
        return match css_value_type(inner) {
            Some("length") => Some("text-size"),
            Some("color") => Some("text-color"),
            _ => None,
        };
    }
    Some("text-color")
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

pub(crate) fn conflict_key(class: &str) -> Option<&'static str> {
    if class.starts_with("text-") { return text_category(class); }
    if class.starts_with("flex-") { return flex_category(class); }
    if class.starts_with("border-") { return border_category(class); }
    if class.starts_with("font-") { return font_category(class); }

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
