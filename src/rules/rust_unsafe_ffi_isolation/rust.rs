//! rust-unsafe-ffi-isolation backend.
//!
//! Matches `extern "C"` / `extern "system"` blocks (`foreign_mod_item`)
//! whose ABI is a genuine foreign calling convention, then walks the
//! ancestor chain looking for a `mod_item` named `sys`, `ffi`, `raw`,
//! or `bindings`. When no such wrapper exists, the FFI block is exposed
//! at the top of the file and gets flagged.
//!
//! A `foreign_mod_item` with a non-foreign ABI string (e.g.
//! `extern "SQL"`, a DSL marker for diesel's `#[declare_sql_function]`
//! proc macro) is not a C/foreign interface and is left untouched.
//!
//! A block carrying an outer `#[wasm_bindgen]` attribute is also left
//! untouched: the `wasm_bindgen` proc macro rewrites the block into safe
//! JavaScript interop with no C calling convention or raw-pointer ABI,
//! and its items must stay in the surrounding module's scope.
//!
//! A block nested inside a function body (a `function_item` ancestor) is
//! left untouched as well: it is already scoped to that one function, more
//! isolated than any module-level `mod sys`, so only module-scope blocks
//! are flagged.

use crate::diagnostic::{Diagnostic, Severity};

const SAFE_MOD_NAMES: &[&str] = &["sys", "ffi", "raw", "bindings"];

/// Proc macros that rewrite an `extern` block's semantics into safe,
/// non-C interop. A block annotated with one of these is not foreign FFI
/// and cannot be relocated into a `mod sys`/`ffi` wrapper.
const BINDING_MACRO_ATTRS: &[&str] = &["wasm_bindgen"];

/// Genuine foreign calling conventions accepted by rustc. A bare
/// `extern { ... }` carries no ABI string and implicitly means `"C"`,
/// so it is treated as foreign too. Any other ABI string (`"SQL"`,
/// `"Rust"`, …) is a DSL marker or the native ABI, not foreign FFI.
const FOREIGN_ABIS: &[&str] = &[
    "C",
    "C-unwind",
    "system",
    "system-unwind",
    "cdecl",
    "cdecl-unwind",
    "stdcall",
    "stdcall-unwind",
    "fastcall",
    "fastcall-unwind",
    "thiscall",
    "thiscall-unwind",
    "vectorcall",
    "vectorcall-unwind",
    "win64",
    "win64-unwind",
    "sysv64",
    "sysv64-unwind",
    "aapcs",
    "aapcs-unwind",
    "efiapi",
];

/// Reads the ABI string of a `foreign_mod_item` from its
/// `extern_modifier`. Returns `None` for a bare `extern { ... }`, which
/// implicitly uses the `"C"` ABI.
fn abi<'a>(node: &tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    let modifier = node
        .children(&mut cursor)
        .find(|child| child.kind() == "extern_modifier")?;
    let mut modifier_cursor = modifier.walk();
    let literal = modifier
        .children(&mut modifier_cursor)
        .find(|child| child.kind() == "string_literal")?;
    let mut c = literal.walk();
    let content = literal
        .children(&mut c)
        .find(|child| child.kind() == "string_content")?;
    content.utf8_text(source).ok()
}

/// Whether the `foreign_mod_item` carries an outer attribute naming a
/// binding-generation proc macro (see `BINDING_MACRO_ATTRS`). Outer
/// attributes are preceding siblings of the block, optionally separated
/// from it by comments, so the scan walks back over `attribute_item`
/// siblings and skips interleaved comments.
fn has_binding_macro_attr(node: &tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = node.prev_sibling();
    while let Some(prev) = sibling {
        match prev.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if attr_path_head(&prev, source)
                    .is_some_and(|head| BINDING_MACRO_ATTRS.contains(&head))
                {
                    return true;
                }
            }
            _ => break,
        }
        sibling = prev.prev_sibling();
    }
    false
}

/// The leading path identifier of an `attribute_item`, e.g. `wasm_bindgen`
/// for both `#[wasm_bindgen]` and `#[wasm_bindgen(extends = Window)]`.
/// Returns `None` when the attribute's path is not a bare identifier (a
/// scoped path like `crate::foo` never names a binding macro here).
fn attr_path_head<'a>(attribute_item: &tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut item_cursor = attribute_item.walk();
    let attribute = attribute_item
        .children(&mut item_cursor)
        .find(|child| child.kind() == "attribute")?;
    let path = attribute.named_child(0)?;
    if path.kind() != "identifier" {
        return None;
    }
    path.utf8_text(source).ok()
}

crate::ast_check! { on ["foreign_mod_item"] => |node, source, ctx, diagnostics|
    // `#[wasm_bindgen]` and friends turn the block into safe JS interop,
    // not C FFI, and the items cannot move into a `mod sys`/`ffi` wrapper.
    if has_binding_macro_attr(&node, source) {
        return;
    }

    // A bare `extern { ... }` (no ABI string) is implicitly `"C"` FFI.
    if let Some(abi) = abi(&node, source)
        && !FOREIGN_ABIS.contains(&abi)
    {
        return;
    }

    let mut current = node.parent();
    while let Some(ancestor) = current {
        // A function-scoped `extern` block is already isolated to one
        // function — invisible outside it, more isolated than `mod sys`.
        // Relocating it into a module would *decrease* isolation, so only
        // module-scope blocks need the wrapper. The block's own foreign `fn`
        // declarations are children, never ancestors, so this matches only an
        // enclosing function body.
        if ancestor.kind() == "function_item" {
            return;
        }
        if ancestor.kind() == "mod_item"
            && let Some(name) = ancestor.child_by_field_name("name")
            && let Ok(text) = name.utf8_text(source)
            && SAFE_MOD_NAMES.contains(&text)
        {
            return;
        }
        current = ancestor.parent();
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Isolate `extern \"C\"` inside `mod sys { ... }` or `mod ffi { ... }`.".into(),
        Severity::Warning,
    ));
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_extern_c_at_root() {
        assert_eq!(run(r#"extern "C" { fn foo(); }"#).len(), 1);
    }

    #[test]
    fn allows_extern_c_in_sys_mod() {
        assert!(run("mod sys {\n    extern \"C\" { fn foo(); }\n}").is_empty());
    }

    #[test]
    fn allows_extern_c_in_ffi_mod() {
        assert!(run("mod ffi {\n    extern \"C\" { fn bar(); }\n}").is_empty());
    }

    #[test]
    fn flags_bare_extern_block() {
        // A bare `extern { ... }` is implicitly the `"C"` ABI.
        assert_eq!(run(r#"extern { fn foo(); }"#).len(), 1);
    }

    #[test]
    fn allows_wasm_bindgen_extern_block() {
        // `#[wasm_bindgen]` rewrites the block into safe JS interop, not C FFI.
        assert!(run("#[wasm_bindgen]\nextern \"C\" {\n    pub type WindowExt;\n}").is_empty());
    }

    #[test]
    fn allows_wasm_bindgen_extern_block_with_inner_attrs() {
        // The full winit repro shape: inner `#[wasm_bindgen(...)]` attributes
        // on the foreign items must not defeat the outer-attribute exemption.
        let src = "#[wasm_bindgen]\n\
                   extern \"C\" {\n    \
                   #[wasm_bindgen(extends = Window)]\n    \
                   pub(crate) type WindowExt;\n    \
                   #[wasm_bindgen(method, getter, js_name = getScreenDetails)]\n    \
                   fn has_screen_details(this: &WindowExt) -> JsValue;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_wasm_bindgen_extern_block_with_outer_args() {
        // The outer attribute may itself carry arguments.
        let src = "#[wasm_bindgen(module = \"/js/shim.js\")]\nextern \"C\" {\n    fn shim();\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_wasm_bindgen_extern_block_with_comment() {
        // A comment may sit between the attribute and the block.
        let src = "#[wasm_bindgen]\n// JS bindings\nextern \"C\" {\n    fn shim();\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_link_extern_block() {
        // `#[link]` is not a binding macro: a genuine C FFI block stays flagged.
        assert_eq!(
            run("#[link(name = \"foo\")]\nextern \"C\" {\n    fn foo();\n}").len(),
            1
        );
    }

    #[test]
    fn allows_extern_sql_dsl_marker() {
        // diesel's `#[declare_sql_function]` uses `extern "SQL"` as a DSL
        // marker, not a foreign function interface.
        let src = "#[crate::declare_sql_function]\n\
                   extern \"SQL\" {\n    fn lower(x: VarChar) -> VarChar;\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_function_scoped_extern_block() {
        // cxx `src/result.rs`: an `extern "C"` block inside a function body is
        // already isolated to that one function and cannot leak further.
        let src = "unsafe fn to_c_error() -> Result {\n    \
                   extern \"C\" {\n        \
                   fn error(ptr: *const u8, len: usize) -> NonNull<u8>;\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_extern_block_in_nested_function() {
        // cxx `tests/test.rs`: an `extern "C"` block nested two functions deep.
        let src = "fn outer() {\n    fn inner() {\n        \
                   extern \"C\" {\n            fn cxx_run_test() -> *const i8;\n        }\n    }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_extern_block_in_impl_method() {
        // A method body is a function body: the block is function-scoped.
        let src = "impl Foo {\n    fn m(&self) {\n        \
                   extern \"C\" {\n            fn g();\n        }\n    }\n}";
        assert!(run(src).is_empty());
    }
}
