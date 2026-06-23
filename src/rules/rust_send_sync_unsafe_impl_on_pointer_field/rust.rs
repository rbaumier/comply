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
use crate::rules::rust_helpers::has_adjacent_safety_comment;

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
        // A documented unsafe impl spells out the externally-upheld invariant
        // (the `// SAFETY:` convention `rust-undocumented-unsafe` enforces);
        // defer to the author's justification.
        if has_adjacent_safety_comment(node, ctx.source) {
            return;
        }
        // A conditional auto-trait forwarding impl — `unsafe impl<T: ?Sized +
        // Send> Send for W<T>` — only asserts the trait when the generic param
        // already carries it; that is the sound hand-rolled lock / cell-wrapper
        // signature, not a hand-waved promise.
        if forwards_auto_trait_conditionally(node, last_segment, source) {
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
        // A `Sync` impl conditioned on `<T: Send>` (rather than `<T: Sync>`)
        // is the sound signature of a hand-rolled synchronization primitive:
        // the struct guards its `UnsafeCell` with an atomic, so the wrapped
        // value only needs to be `Send` to be shared. Recognise that shape —
        // a `Sync` impl whose bounds forward `Send` over a struct that also
        // carries an atomic field — and defer to the author.
        if last_segment == "Sync"
            && forwards_auto_trait_conditionally(node, "Send", source)
            && struct_has_atomic_field(struct_node, source)
        {
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

/// True if the impl forwards the auto-trait `trait_name` (`Send` / `Sync`)
/// conditionally on a generic parameter — i.e. the impl's generic bounds
/// already require `trait_name`, as in `unsafe impl<T: ?Sized + Send> Send for
/// W<T>`. The `type_parameters` field (`<…>`) holds the generics and their
/// bounds; we check whether its text mentions `trait_name`.
///
/// Requiring the trait name inside the bounds is what makes this safe: a sound
/// forwarding impl (`<T: Send>`) is skipped, but an unconditional generic impl
/// (`unsafe impl<T> Send for W<T>`, whose `type_parameters` text is just `<T>`)
/// keeps being flagged — it asserts `Send` for every `T`, including non-`Send`
/// ones, which is unsound.
fn forwards_auto_trait_conditionally(
    node: tree_sitter::Node,
    trait_name: &str,
    source: &[u8],
) -> bool {
    let Some(type_params) = node.child_by_field_name("type_parameters") else {
        return false;
    };
    let Ok(text) = type_params.utf8_text(source) else {
        return false;
    };
    text.split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|word| word == trait_name)
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

/// True if the struct has a direct field whose own type is one of the standard
/// atomics (`AtomicBool`, `AtomicUsize`, …) — the interior lock that lets a
/// hand-rolled primitive be `Sync` while only requiring its payload to be
/// `Send`. Only the field's own type counts: an atomic nested inside another
/// field's payload (`UnsafeCell<AtomicUsize>`) is the guarded value, not a
/// guard, and must not exempt the impl.
fn struct_has_atomic_field(struct_node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = struct_node.walk();
    for child in struct_node.named_children(&mut cursor) {
        match child.kind() {
            "field_declaration_list" => {
                if list_has_atomic_field(child, source) {
                    return true;
                }
            }
            "ordered_field_declaration_list" => {
                if ordered_has_atomic_field(child, source) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn list_has_atomic_field(list: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = list.walk();
    for field in list.named_children(&mut cursor) {
        if field.kind() != "field_declaration" {
            continue;
        }
        let Some(ty) = field.child_by_field_name("type") else {
            continue;
        };
        if is_atomic_type(ty, source) {
            return true;
        }
    }
    false
}

fn ordered_has_atomic_field(list: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = list.walk();
    for ty in list.named_children(&mut cursor) {
        if is_atomic_type(ty, source) {
            return true;
        }
    }
    false
}

/// True if `node` names a standard atomic type. The field is written as a plain
/// `type_identifier` (`AtomicBool`) or a `scoped_type_identifier`
/// (`atomic::AtomicBool`); in both cases the last path segment is the type name.
fn is_atomic_type(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else {
        return false;
    };
    let last = text.rsplit("::").next().unwrap_or(text).trim();
    last.starts_with("Atomic")
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
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
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

    #[test]
    fn allows_documented_unsafe_impl_with_safety_comment() {
        let src = "struct Page { inner: UnsafeCell<u32> }\n\
                   // SAFETY: atomic page flags serialize concurrent modifications.\n\
                   unsafe impl Send for Page {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_conditional_send_forwarding() {
        let src = "struct SpinLock<T> { value: UnsafeCell<T> }\n\
                   unsafe impl<T: ?Sized + Send> Send for SpinLock<T> {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_conditional_sync_forwarding() {
        let src = "struct SpinLock<T> { value: UnsafeCell<T> }\n\
                   unsafe impl<T: ?Sized + Sync> Sync for SpinLock<T> {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_unconditional_generic_impl() {
        // `<T>` has no `Send` bound, so the impl asserts `Send` for every `T`
        // including non-`Send` ones — unsound, must stay flagged.
        let src = "struct W<T> { c: UnsafeCell<T> }\nunsafe impl<T> Send for W<T> {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_sync_impl_send_bound_on_atomic_guarded_primitive() {
        // embedded-hal `AtomicCell`: the `AtomicBool` guards the `UnsafeCell`,
        // so `unsafe impl<BUS: Send> Sync` is sound even though the bound is
        // `Send`, not `Sync`.
        let src = "struct AtomicCell<BUS> { bus: UnsafeCell<BUS>, busy: AtomicBool }\n\
                   unsafe impl<BUS: Send> Sync for AtomicCell<BUS> {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_sync_send_bound_without_atomic_field() {
        // No atomic guard — a `Sync` impl bounded only on `Send` is unsound
        // here, so it must stay flagged.
        let src = "struct W<T> { c: UnsafeCell<T> }\n\
                   unsafe impl<T: Send> Sync for W<T> {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_atomic_nested_in_cell_payload() {
        // The `AtomicUsize` is the guarded payload, not a sibling guard field —
        // there is no interior lock, so the impl is unsound and stays flagged.
        let src = "struct W<T> { c: UnsafeCell<AtomicUsize> }\n\
                   unsafe impl<T: Send> Sync for W<T> {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn non_safety_comment_does_not_exempt() {
        let src = "struct S { c: Cell<u32> }\n\
                   // just a normal comment\n\
                   unsafe impl Sync for S {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_safety_comment_above_cfg_attribute() {
        // embassy-sync AtomicWaker: the `// SAFETY:` comment sits above a
        // `#[cfg(...)]` gating the `unsafe impl`. The comment still documents it.
        let src = "struct AtomicWaker { waker: UnsafeCell<u32> }\n\
                   // SAFETY: access to the cell is serialized through the state machine.\n\
                   #[cfg(target_has_atomic = \"32\")]\n\
                   unsafe impl Send for AtomicWaker {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_safety_comment_above_stacked_attributes() {
        let src = "struct AtomicWaker { waker: UnsafeCell<u32> }\n\
                   // SAFETY: access to the cell is serialized through the state machine.\n\
                   #[cfg(target_has_atomic = \"32\")]\n\
                   #[allow(clippy::non_send_fields_in_send_ty)]\n\
                   unsafe impl Sync for AtomicWaker {}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_cfg_attribute_without_safety_comment() {
        // Only a `#[cfg]`, no SAFETY comment anywhere above — stays flagged.
        let src = "struct AtomicWaker { waker: UnsafeCell<u32> }\n\
                   #[cfg(target_has_atomic = \"32\")]\n\
                   unsafe impl Send for AtomicWaker {}";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn still_flags_non_safety_comment_above_cfg_attribute() {
        // A plain comment above the attribute is not a SAFETY justification.
        let src = "struct AtomicWaker { waker: UnsafeCell<u32> }\n\
                   // just a normal comment\n\
                   #[cfg(target_has_atomic = \"32\")]\n\
                   unsafe impl Send for AtomicWaker {}";
        assert_eq!(run_on(src).len(), 1);
    }
}
