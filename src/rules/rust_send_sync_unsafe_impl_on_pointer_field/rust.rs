//! rust-send-sync-unsafe-impl-on-pointer-field backend.
//!
//! For every `unsafe impl Send for X` / `unsafe impl Sync for X` we walk
//! the file looking for a `struct_item` whose name is `X`. If that struct
//! has a field whose type is `Cell<…>`, `RefCell<…>`, or `UnsafeCell<…>`
//! we flag the impl. Raw pointers (`*const T` / `*mut T`) are `Send + Sync`
//! by default in Rust and are not flagged. We do not chase down cross-file
//! definitions — comply only inspects one file at a time and the
//! false-positive rate of guessing about other files is too high.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["impl_item"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source) else {
            return;
        };
        if !text.trim_start().starts_with("unsafe impl") {
            return;
        }
        // The `trait` field on an impl_item holds the trait being
        // implemented (Send / Sync). The `type` field holds the type.
        let Some(trait_node) = node.child_by_field_name("trait") else {
            return;
        };
        let Ok(trait_text) = trait_node.utf8_text(source) else {
            return;
        };
        let last_segment = trait_text.rsplit("::").next().unwrap_or(trait_text);
        if last_segment != "Send" && last_segment != "Sync" {
            return;
        }
        let Some(type_node) = node.child_by_field_name("type") else {
            return;
        };
        let Ok(target_name) = type_node.utf8_text(source) else {
            return;
        };
        // Strip generics: `Foo<T>` -> `Foo`.
        let target_base = target_name.split('<').next().unwrap_or(target_name).trim();
        let target_base = target_base.rsplit("::").next().unwrap_or(target_base);

        // Walk the file root looking for a matching struct_item.
        let mut root = node;
        while let Some(p) = root.parent() {
            root = p;
        }
        let Some(struct_node) = find_struct(root, source, target_base) else {
            return;
        };
        if !struct_has_unsync_field(struct_node, source) {
            return;
        }
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &node,
            "rust-send-sync-unsafe-impl-on-pointer-field",
            format!(
                "`unsafe impl {last_segment} for {target_base}` — but \
                 `{target_base}` holds a `Cell` / `RefCell` / `UnsafeCell` \
                 field. Wrap with `Mutex`, `Atomic*`, or `parking_lot` \
                 instead of asserting thread safety by hand."
            ),
            Severity::Error,
        ));
    }
}

fn find_struct<'a>(
    root: tree_sitter::Node<'a>,
    source: &[u8],
    target: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut stack = vec![root];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "struct_item"
            && let Some(name) = cur.child_by_field_name("name")
            && let Ok(t) = name.utf8_text(source)
            && t == target
        {
            return Some(cur);
        }
        let mut cursor = cur.walk();
        for child in cur.children(&mut cursor) {
            stack.push(child);
        }
    }
    None
}

fn struct_has_unsync_field(struct_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = struct_node.walk();
    for child in struct_node.named_children(&mut cursor) {
        match child.kind() {
            "field_declaration_list" => {
                if list_has_unsync_field(child, source) {
                    return true;
                }
            }
            "ordered_field_declaration_list" => {
                if ordered_has_unsync_field(child, source) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn list_has_unsync_field(list: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = list.walk();
    for field in list.named_children(&mut cursor) {
        if field.kind() != "field_declaration" {
            continue;
        }
        let Some(ty) = field.child_by_field_name("type") else {
            continue;
        };
        if is_unsync_type(ty, source) {
            return true;
        }
    }
    false
}

fn ordered_has_unsync_field(list: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = list.walk();
    for ty in list.named_children(&mut cursor) {
        if is_unsync_type(ty, source) {
            return true;
        }
    }
    false
}

fn is_unsync_type(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() == "generic_type"
        && let Some(t) = node.child_by_field_name("type")
        && let Ok(text) = t.utf8_text(source)
    {
        let last = text.rsplit("::").next().unwrap_or(text);
        if last == "Cell" || last == "RefCell" || last == "UnsafeCell" {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn allows_unsafe_impl_send_with_raw_ptr() {
        let src = "struct S { p: *mut u8 }\nunsafe impl Send for S {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unsafe_impl_sync_with_raw_pointer() {
        let src = "struct Buffer { ptr: *mut u8 }\nunsafe impl Sync for Buffer {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_sync_for_struct_with_cell() {
        let src = "struct S { c: Cell<u32> }\nunsafe impl Sync for S {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_send_for_struct_with_refcell() {
        let src = "struct S { c: RefCell<u32> }\nunsafe impl Send for S {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_unsafecell() {
        let src = "struct S { c: UnsafeCell<u32> }\nunsafe impl Sync for S {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_unsafe_impl_when_struct_has_only_safe_fields() {
        let src = "struct S { x: u32 }\nunsafe impl Send for S {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn does_not_flag_safe_impl() {
        let src = "struct S { p: *mut u8 }\nimpl Default for S { fn default() -> Self { S { p: std::ptr::null_mut() } } }";
        assert!(run_on(src).is_empty());
    }
}
