//! tailwind-classnames-order backend — flag `className` / `class` strings
//! whose utilities are not grouped in the recommended category order.
//!
//! Rather than enforcing a fully-deterministic ordering (which requires a
//! complete class table to match `prettier-plugin-tailwindcss`), this rule
//! checks the *coarse* category order: `layout → flex/grid → spacing →
//! sizing → typography → backgrounds → borders → effects → interactivity`.
//! A diagnostic is emitted when two utilities belonging to different
//! categories appear out of order relative to each other. Classes with
//! unknown categories are ignored.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Coarse ordering groups. Lower index = should appear earlier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    Layout,       // position, display, float, isolation, z-index, overflow
    FlexGrid,     // flex-*, grid-*, gap-*, justify-*, items-*, self-*, order-*
    Spacing,      // m-*, p-*, space-x-*, space-y-*
    Sizing,       // w-*, h-*, min-w-*, min-h-*, max-w-*, max-h-*, size-*
    Typography,   // text-*, font-*, leading-*, tracking-*, uppercase, italic, ...
    Backgrounds,  // bg-*
    Borders,      // border-*, rounded-*, outline-*, ring-*, divide-*
    Effects,      // shadow-*, opacity-*, blur-*, backdrop-*
    Transitions,  // transition-*, duration-*, ease-*, animate-*, transform-*, rotate-*, scale-*, translate-*
    Interactivity, // cursor-*, select-*, resize-*, pointer-events-*, appearance-*
}

const LAYOUT_CLASSES: &[&str] = &[
    "block", "inline", "inline-block", "flex", "inline-flex", "grid", "inline-grid",
    "hidden", "contents", "table", "flow-root", "list-item",
    "static", "fixed", "absolute", "relative", "sticky",
    "visible", "invisible", "collapse", "isolate", "isolation-auto",
    "float-left", "float-right", "float-none",
    "clear-left", "clear-right", "clear-both", "clear-none",
];

const LAYOUT_PREFIXES: &[&str] = &[
    "z-", "overflow-", "overscroll-", "inset-", "top-", "right-", "bottom-", "left-",
    "container",
];

const FLEXGRID_PREFIXES: &[&str] = &[
    "flex-", "grid-", "gap-", "justify-", "items-", "content-", "self-", "place-",
    "order-", "col-", "row-", "auto-cols-", "auto-rows-", "basis-",
];

const FLEXGRID_CLASSES: &[&str] = &["flex-row", "flex-col", "flex-wrap", "flex-nowrap"];

const SPACING_PREFIXES: &[&str] = &[
    "p-", "px-", "py-", "pt-", "pr-", "pb-", "pl-", "ps-", "pe-",
    "m-", "mx-", "my-", "mt-", "mr-", "mb-", "ml-", "ms-", "me-",
    "space-x-", "space-y-",
];

const SIZING_PREFIXES: &[&str] = &[
    "w-", "h-", "min-w-", "min-h-", "max-w-", "max-h-", "size-",
];

const TYPOGRAPHY_PREFIXES: &[&str] = &[
    "text-", "font-", "leading-", "tracking-", "whitespace-", "break-",
    "line-clamp-", "list-", "decoration-", "underline-",
];

const TYPOGRAPHY_CLASSES: &[&str] = &[
    "italic", "not-italic", "uppercase", "lowercase", "capitalize", "normal-case",
    "underline", "overline", "line-through", "no-underline", "truncate",
    "antialiased", "subpixel-antialiased",
];

const BACKGROUND_PREFIXES: &[&str] = &["bg-", "from-", "via-", "to-"];

const BORDER_PREFIXES: &[&str] = &[
    "border", "rounded", "outline", "ring", "divide-",
];

const EFFECT_PREFIXES: &[&str] = &["shadow", "opacity-", "blur", "brightness-", "backdrop-", "mix-blend-"];

const TRANSITION_PREFIXES: &[&str] = &[
    "transition", "duration-", "ease-", "delay-", "animate-",
    "transform", "rotate-", "scale-", "translate-", "skew-", "origin-",
];

const INTERACTIVITY_PREFIXES: &[&str] = &[
    "cursor-", "select-", "resize", "pointer-events-", "appearance-",
    "touch-", "will-change-", "scroll-",
];

/// Best-effort classification of a base (un-variant-prefixed) class into a group.
fn classify(base: &str) -> Option<Group> {
    if LAYOUT_CLASSES.contains(&base) {
        return Some(Group::Layout);
    }
    if FLEXGRID_CLASSES.contains(&base) {
        return Some(Group::FlexGrid);
    }
    if TYPOGRAPHY_CLASSES.contains(&base) {
        return Some(Group::Typography);
    }
    // Match prefix tables in group order, but check longer-specific groups first
    // when a class could match multiple (e.g. `scroll-m-2` should be SCROLL, not spacing).
    if has_prefix(base, INTERACTIVITY_PREFIXES) {
        return Some(Group::Interactivity);
    }
    if has_prefix(base, TRANSITION_PREFIXES) {
        return Some(Group::Transitions);
    }
    if has_prefix(base, EFFECT_PREFIXES) {
        return Some(Group::Effects);
    }
    if has_prefix(base, BORDER_PREFIXES) {
        return Some(Group::Borders);
    }
    if has_prefix(base, BACKGROUND_PREFIXES) {
        return Some(Group::Backgrounds);
    }
    if has_prefix(base, TYPOGRAPHY_PREFIXES) {
        return Some(Group::Typography);
    }
    if has_prefix(base, SIZING_PREFIXES) {
        return Some(Group::Sizing);
    }
    if has_prefix(base, SPACING_PREFIXES) {
        return Some(Group::Spacing);
    }
    if has_prefix(base, FLEXGRID_PREFIXES) {
        return Some(Group::FlexGrid);
    }
    if has_prefix(base, LAYOUT_PREFIXES) {
        return Some(Group::Layout);
    }
    None
}

/// True if `class` starts with any of `prefixes`. Prefixes ending with `-`
/// require that dash; bare prefixes (e.g. `border`, `shadow`) match the exact
/// word or a dashed extension.
fn has_prefix(class: &str, prefixes: &[&str]) -> bool {
    for p in prefixes {
        if p.ends_with('-') {
            if class.starts_with(p) {
                return true;
            }
        } else if class == *p || class.starts_with(&format!("{p}-")) {
            return true;
        }
    }
    false
}

/// Strip Tailwind variant prefix (`md:`, `hover:`, `dark:hover:`) and `!` modifier.
fn strip_prefixes(class: &str) -> &str {
    let bare = class.rsplit(':').next().unwrap_or(class);
    bare.strip_prefix('!').unwrap_or(bare)
}

/// Extract class-string values from `className="..."` or `class="..."`.
fn extract_class_strings(line: &str) -> Vec<&str> {
    let mut results = Vec::new();
    for attr in ["className=\"", "class=\""] {
        let mut search_from = 0;
        while let Some(start) = line[search_from..].find(attr) {
            let abs_start = search_from + start + attr.len();
            if let Some(end) = line[abs_start..].find('"') {
                results.push(&line[abs_start..abs_start + end]);
            }
            search_from = abs_start;
        }
    }
    results
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for class_str in extract_class_strings(line) {
                let classes: Vec<&str> = class_str.split_whitespace().collect();
                if classes.len() < 2 {
                    continue;
                }
                // Classify each class (ignore unknowns).
                let groups: Vec<(usize, Group, &str)> = classes
                    .iter()
                    .enumerate()
                    .filter_map(|(i, c)| classify(strip_prefixes(c)).map(|g| (i, g, *c)))
                    .collect();

                // Find the first pair where a later class has a strictly smaller group.
                for window in groups.windows(2) {
                    let (_, prev_group, prev_class) = window[0];
                    let (_, cur_group, cur_class) = window[1];
                    if cur_group < prev_group {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: idx + 1,
                            column: 1,
                            rule_id: "tailwind-classnames-order".into(),
                            message: format!(
                                "Tailwind classes out of order: `{cur_class}` ({cur_group:?}) should appear before `{prev_class}` ({prev_group:?})."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                        // One diagnostic per class string is enough — reordering one
                        // pair may fix several downstream pairs.
                        break;
                    }
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.tsx"), source))
    }

    #[test]
    fn flags_spacing_before_layout() {
        // `flex` (FlexGrid) after `p-2` (Spacing) is out of order.
        let diags = run(r#"<div className="p-2 flex" />"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_bg_before_sizing() {
        // `w-4` (Sizing) after `bg-red-500` (Backgrounds) is out of order.
        let diags = run(r#"<div className="bg-red-500 w-4" />"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_typography_before_spacing() {
        let diags = run(r#"<div className="text-lg p-2" />"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_canonical_order() {
        assert!(run(r#"<div className="flex p-2 w-4 text-lg bg-red-500" />"#).is_empty());
    }

    #[test]
    fn allows_single_class() {
        assert!(run(r#"<div className="flex" />"#).is_empty());
    }

    #[test]
    fn allows_all_same_group() {
        assert!(run(r#"<div className="p-2 px-4 mt-1" />"#).is_empty());
    }

    #[test]
    fn ignores_unknown_classes() {
        // `custom-thing` is unknown; `flex p-2` alone is in order.
        assert!(run(r#"<div className="flex custom-thing p-2" />"#).is_empty());
    }
}
