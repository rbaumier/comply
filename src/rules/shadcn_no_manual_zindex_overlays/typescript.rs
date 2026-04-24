//! Flag `className` containing `z-<digits>` on overlay components.
//!
//! We check the first identifier segment of the JSX tag (e.g. `Dialog`
//! in `Dialog.Content` or `DialogContent`) against a list of overlay
//! primitives. Both dotted (`Dialog.Content`) and flat
//! (`DialogContent`) styles are covered.

use crate::diagnostic::{Diagnostic, Severity};

const OVERLAY_TAGS: &[&str] = &[
    "Dialog",
    "DialogContent",
    "DialogOverlay",
    "Sheet",
    "SheetContent",
    "SheetOverlay",
    "Drawer",
    "DrawerContent",
    "DrawerOverlay",
    "AlertDialog",
    "AlertDialogContent",
    "AlertDialogOverlay",
    "DropdownMenu",
    "DropdownMenuContent",
    "Popover",
    "PopoverContent",
    "Tooltip",
    "TooltipContent",
];

fn is_zindex_class(class: &str) -> bool {
    let utility = class.rsplit(':').next().unwrap_or(class);
    let utility = utility.trim_start_matches('!').trim_start_matches('-');
    let Some(rest) = utility.strip_prefix("z-") else {
        return false;
    };
    // `z-10`, `z-50`, `z-[100]` all count. `z-auto` is harmless.
    if rest == "auto" {
        return false;
    }
    rest.chars().all(|c| c.is_ascii_digit() || c == '[' || c == ']')
        && rest.chars().any(|c| c.is_ascii_digit())
}

fn tag_is_overlay(tag: &str) -> bool {
    // `Dialog.Content` → first segment `Dialog`, full match `Dialog.Content`.
    // Accept either the full dotted form or the first segment alone.
    if OVERLAY_TAGS.contains(&tag) {
        return true;
    }
    let first = tag.split('.').next().unwrap_or(tag);
    OVERLAY_TAGS.contains(&first)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "jsx_opening_element" && kind != "jsx_self_closing_element" {
        return;
    }
    let Some(tag) = crate::rules::jsx::jsx_element_tag_name(node, source) else {
        return;
    };
    if !tag_is_overlay(tag) {
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "jsx_attribute" {
            continue;
        }
        if crate::rules::jsx::jsx_attribute_name(child, source) != Some("className") {
            continue;
        }
        let Some(value) = crate::rules::jsx::jsx_attribute_string_value(child, source) else {
            continue;
        };
        if value.split_ascii_whitespace().any(is_zindex_class) {
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &child,
                super::META.id,
                format!("`z-*` on `{tag}` fights shadcn's overlay stacking — drop the z-index utility."),
                Severity::Warning,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_z_on_dialog_content() {
        assert_eq!(run(r#"const x = <DialogContent className="z-50">x</DialogContent>;"#).len(), 1);
    }

    #[test]
    fn flags_z_on_dotted_popover() {
        assert_eq!(run(r#"const x = <Popover.Content className="p-4 z-[999]">x</Popover.Content>;"#).len(), 1);
    }

    #[test]
    fn allows_z_on_non_overlay() {
        assert!(run(r#"const x = <div className="z-10">x</div>;"#).is_empty());
    }

    #[test]
    fn allows_overlay_without_z() {
        assert!(run(r#"const x = <DialogContent className="p-4">x</DialogContent>;"#).is_empty());
    }

    #[test]
    fn allows_z_auto() {
        assert!(run(r#"const x = <DialogContent className="z-auto">x</DialogContent>;"#).is_empty());
    }
}
