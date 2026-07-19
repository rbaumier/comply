//! a11y-click-events-have-key-events

mod oxc_typescript;
#[cfg(test)]
mod react;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "a11y-click-events-have-key-events",
    description: "Elements with `onClick` must also have a keyboard event handler.",
    remediation: "Add `onKeyDown`, `onKeyUp`, or `onKeyPress` alongside `onClick` for keyboard accessibility.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["accessibility"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    let mut backends = vec![
        (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
        (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
    ];
    backends.push((Language::Vue, Backend::Text(Box::new(vue::Check))));
    RuleDef {
        meta: META,
        backends,
    }
}

/// Native HTML tags with built-in keyboard activation — `Space`/`Enter`
/// fire `click` automatically, so a paired keyboard handler is redundant.
const NATIVE_INTERACTIVE_TAGS: &[&str] =
    &["button", "a", "input", "select", "textarea", "summary", "details"];

/// Component-name roots whose `*Item` descendants expose a native
/// `menuitem`/`option`-style role (full keyboard + typeahead navigation by
/// construction). Matched together with the `Item` suffix so only items of an
/// interactive container are exempt — `ListItem`/`GridItem`/`AccordionItem`
/// stay flagged.
const INTERACTIVE_ITEM_CONTAINERS: &[&str] = &[
    "Menu",
    "Select",
    "Combobox",
    "Listbox",
    "Command",
    "Dropdown",
    "Autocomplete",
];

/// Whether a tag has built-in keyboard activation and is therefore exempt from
/// the click/keyboard-handler pairing requirement. Shared by the OXC (TSX/JSX)
/// and Vue backends so the two cannot drift on the tag-level decision.
fn tag_has_native_keyboard_support(tag: &str) -> bool {
    // Lowercase identifier = native HTML tag.
    if NATIVE_INTERACTIVE_TAGS.contains(&tag) {
        return true;
    }
    // Uppercase identifier = component. Treat names ending in `Button`
    // (Button, IconButton, PrimaryButton, …) as wrapping a real
    // `<button>` — same keyboard semantics apply.
    if tag.ends_with("Button") {
        return true;
    }
    // Link components (Link, NavLink, RouterLink, …) render an `<a href>`;
    // Enter activates them natively, like the native `<a>` above.
    if tag.ends_with("Link") {
        return true;
    }
    // Menu/select/listbox/combobox item components carry a `menuitem`/`option`
    // role with built-in keyboard navigation (DropdownMenuItem, SelectItem,
    // ContextMenuItem, DropdownMenuRadioItem, …). `Option` is the bare case.
    if tag == "Option" {
        return true;
    }
    tag.ends_with("Item") && INTERACTIVE_ITEM_CONTAINERS.iter().any(|root| tag.contains(root))
}
