mod oxc_typescript;

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "tailwind-classnames-order",
    description: "Tailwind classes should follow a canonical category order (layout → spacing → sizing → typography → visual).",
    remediation: "Reorder utility classes to follow the recommended group order. Tools like `prettier-plugin-tailwindcss` or `eslint-plugin-tailwindcss` can auto-fix this.",
    severity: Severity::Warning,
    doc_url: Some("https://github.com/tailwindlabs/prettier-plugin-tailwindcss"),
    categories: &["tailwind"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

/// Coarse ordering groups. Lower index = should appear earlier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Group {
    Layout,
    FlexGrid,
    Spacing,
    Sizing,
    Typography,
    Backgrounds,
    Borders,
    Effects,
    Transitions,
    Interactivity,
}

const LAYOUT_CLASSES: &[&str] = &[
    "block", "inline", "inline-block", "flex", "inline-flex", "grid",
    "inline-grid", "hidden", "contents", "table", "flow-root", "list-item",
    "static", "fixed", "absolute", "relative", "sticky", "visible",
    "invisible", "collapse", "isolate", "isolation-auto", "float-left",
    "float-right", "float-none", "clear-left", "clear-right", "clear-both",
    "clear-none",
];
const LAYOUT_PREFIXES: &[&str] = &[
    "z-", "overflow-", "overscroll-", "inset-", "top-", "right-",
    "bottom-", "left-", "container",
];
const FLEXGRID_PREFIXES: &[&str] = &[
    "flex-", "grid-", "gap-", "justify-", "items-", "content-", "self-",
    "place-", "order-", "col-", "row-", "auto-cols-", "auto-rows-", "basis-",
];
const FLEXGRID_CLASSES: &[&str] = &["flex-row", "flex-col", "flex-wrap", "flex-nowrap"];
const SPACING_PREFIXES: &[&str] = &[
    "p-", "px-", "py-", "pt-", "pr-", "pb-", "pl-", "ps-", "pe-",
    "m-", "mx-", "my-", "mt-", "mr-", "mb-", "ml-", "ms-", "me-",
    "space-x-", "space-y-",
];
const SIZING_PREFIXES: &[&str] = &["w-", "h-", "min-w-", "min-h-", "max-w-", "max-h-", "size-"];
const TYPOGRAPHY_PREFIXES: &[&str] = &[
    "text-", "font-", "leading-", "tracking-", "whitespace-", "break-",
    "line-clamp-", "list-", "decoration-", "underline-",
];
const TYPOGRAPHY_CLASSES: &[&str] = &[
    "italic", "not-italic", "uppercase", "lowercase", "capitalize",
    "normal-case", "underline", "overline", "line-through", "no-underline",
    "truncate", "antialiased", "subpixel-antialiased",
];
const BACKGROUND_PREFIXES: &[&str] = &["bg-", "from-", "via-", "to-"];
const BORDER_PREFIXES: &[&str] = &["border", "rounded", "outline", "ring", "divide-"];
const EFFECT_PREFIXES: &[&str] = &[
    "shadow", "opacity-", "blur", "brightness-", "backdrop-", "mix-blend-",
];
const TRANSITION_PREFIXES: &[&str] = &[
    "transition", "duration-", "ease-", "delay-", "animate-", "transform",
    "rotate-", "scale-", "translate-", "skew-", "origin-",
];
const INTERACTIVITY_PREFIXES: &[&str] = &[
    "cursor-", "select-", "resize", "pointer-events-", "appearance-",
    "touch-", "will-change-", "scroll-",
];

fn has_prefix(class: &str, prefixes: &[&str]) -> bool {
    for p in prefixes {
        if p.ends_with('-') {
            if class.starts_with(p) { return true; }
        } else if class == *p || class.starts_with(&format!("{p}-")) {
            return true;
        }
    }
    false
}

pub(crate) fn classify(base: &str) -> Option<Group> {
    if LAYOUT_CLASSES.contains(&base) { return Some(Group::Layout); }
    if FLEXGRID_CLASSES.contains(&base) { return Some(Group::FlexGrid); }
    if TYPOGRAPHY_CLASSES.contains(&base) { return Some(Group::Typography); }
    if has_prefix(base, INTERACTIVITY_PREFIXES) { return Some(Group::Interactivity); }
    if has_prefix(base, TRANSITION_PREFIXES) { return Some(Group::Transitions); }
    if has_prefix(base, EFFECT_PREFIXES) { return Some(Group::Effects); }
    if has_prefix(base, BORDER_PREFIXES) { return Some(Group::Borders); }
    if has_prefix(base, BACKGROUND_PREFIXES) { return Some(Group::Backgrounds); }
    if has_prefix(base, TYPOGRAPHY_PREFIXES) { return Some(Group::Typography); }
    if has_prefix(base, SIZING_PREFIXES) { return Some(Group::Sizing); }
    if has_prefix(base, SPACING_PREFIXES) { return Some(Group::Spacing); }
    if has_prefix(base, FLEXGRID_PREFIXES) { return Some(Group::FlexGrid); }
    if has_prefix(base, LAYOUT_PREFIXES) { return Some(Group::Layout); }
    None
}

pub(crate) fn strip_prefixes(class: &str) -> &str {
    let bare = class.rsplit(':').next().unwrap_or(class);
    bare.strip_prefix('!').unwrap_or(bare)
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
