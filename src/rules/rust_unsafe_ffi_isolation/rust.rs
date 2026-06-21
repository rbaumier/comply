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
//!
//! A block with no `#[link(name = ...)]` whose every declared function is
//! namespaced under the owning crate's name (`<crate>_*`) is left untouched:
//! it binds the crate's own hand-written assembly / intrinsic symbols (linked
//! from the same crate's build), not an external library, so it is already at
//! its isolation boundary. Genuine external-library FFI — carrying `#[link]`,
//! or declaring a foreign library's own symbol names — is still flagged.
//!
//! A block inside a dedicated native-binding crate (one declaring
//! `[package].links`, or with a `-sys`/`-cpp` package-name suffix — see
//! [`crate::project::CargoManifest::is_native_binding_crate`]) is left untouched
//! too: the crate's whole purpose is exposing a C/C++ library's `extern "C"`
//! surface, so the block already *is* the isolation layer. Ordinary
//! application/library crates' un-isolated FFI is still flagged. (Bindgen-
//! generated bindings files are skipped earlier by the engine's shared
//! generated-file gate, so they never reach this rule.)

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

/// Whether any outer attribute preceding the `foreign_mod_item` has a leading
/// path identifier satisfying `pred`. Outer attributes are preceding siblings
/// of the block, optionally separated from it by comments, so the scan walks
/// back over `attribute_item` siblings and skips interleaved comments.
fn has_outer_attr(node: &tree_sitter::Node, source: &[u8], pred: impl Fn(&str) -> bool) -> bool {
    let mut sibling = node.prev_sibling();
    while let Some(prev) = sibling {
        match prev.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if attr_path_head(&prev, source).is_some_and(&pred) {
                    return true;
                }
            }
            _ => break,
        }
        sibling = prev.prev_sibling();
    }
    false
}

/// Whether the block carries an outer attribute naming a binding-generation
/// proc macro (see `BINDING_MACRO_ATTRS`).
fn has_binding_macro_attr(node: &tree_sitter::Node, source: &[u8]) -> bool {
    has_outer_attr(node, source, |head| BINDING_MACRO_ATTRS.contains(&head))
}

/// Whether the block carries a `#[link(...)]` attribute — the explicit
/// external-library link marker that genuine foreign-library FFI uses to name
/// the library it binds. Its presence means the block is a real external
/// boundary, so the in-crate-asm exemption must not apply.
fn has_link_attr(node: &tree_sitter::Node, source: &[u8]) -> bool {
    has_outer_attr(node, source, |head| head == "link")
}

/// Whether every foreign function declared in the block is namespaced under
/// the owning crate's name (`<crate>_<rest>`, see
/// [`crate::project::CargoManifest::owns_asm_symbol`]). Such a block declares
/// the crate's own hand-written assembly / intrinsic symbols (linked from the
/// same crate's build), not bindings to an external library, so it is already
/// at its isolation boundary and needs no `mod sys`/`ffi` wrapper. Returns
/// `false` for an empty block (no symbols to attribute) or when any declared
/// symbol is not crate-namespaced.
fn declares_only_own_asm_symbols(
    node: &tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
) -> bool {
    let Some(manifest) = ctx.project.nearest_cargo_manifest(ctx.path) else {
        return false;
    };
    let mut block_cursor = node.walk();
    let Some(body) = node
        .children(&mut block_cursor)
        .find(|child| child.kind() == "declaration_list")
    else {
        return false;
    };
    let mut body_cursor = body.walk();
    let mut saw_fn = false;
    for item in body.children(&mut body_cursor) {
        if item.kind() != "function_signature_item" {
            continue;
        }
        saw_fn = true;
        let owned = item
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(source).ok())
            .is_some_and(|name| manifest.owns_asm_symbol(name));
        if !owned {
            return false;
        }
    }
    saw_fn
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

    // A block with no `#[link(name = ...)]` whose every declared function is
    // namespaced under the owning crate's name (`<crate>_*`) binds the crate's
    // own hand-written assembly / intrinsic symbols, not an external library.
    // It is already at its isolation boundary; a `mod sys`/`ffi` wrapper would
    // add artificial nesting with no safety benefit. Genuine external-library
    // FFI (carrying `#[link]`, or declaring a foreign library's own symbol
    // names) stays flagged.
    if !has_link_attr(&node, source) && declares_only_own_asm_symbols(&node, source, ctx) {
        return;
    }

    // A dedicated native-binding crate (`[package].links`, or a `-sys`/`-cpp`
    // name suffix) exists solely to expose a C/C++ library's `extern "C"`
    // surface — the block itself is the isolation layer, so an inner
    // `mod sys`/`ffi` wrapper would add nesting with no safety benefit.
    if ctx
        .project
        .nearest_cargo_manifest(ctx.path)
        .is_some_and(|manifest| manifest.is_native_binding_crate())
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

    /// Run `source` at `rel_path` inside a temp crate whose `Cargo.toml`
    /// declares `[package] name = "{crate_name}"`, so `owns_asm_symbol`
    /// resolves against a controlled manifest.
    fn run_in_crate(crate_name: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let cargo_toml =
            format!("[package]\nname = \"{crate_name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n");
        run_in_manifest(&cargo_toml, rel_path, source)
    }

    /// Run `source` at `rel_path` inside a temp crate whose `Cargo.toml` is
    /// `cargo_toml` verbatim — lets a test control `[package].links` and other
    /// manifest keys the binding-crate exemption reads.
    fn run_in_manifest(cargo_toml: &str, rel_path: &str, source: &str) -> Vec<Diagnostic> {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), cargo_toml).unwrap();
        let src_path = dir.path().join(rel_path);
        if let Some(parent) = src_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&src_path, source).unwrap();
        crate::rules::test_helpers::run_rule(&Check, source, &src_path)
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

    #[test]
    fn allows_in_crate_asm_symbols() {
        // rav1e `src/asm/x86/mc.rs`: a bare `extern {}` declaring the crate's own
        // hand-written SIMD assembly symbols (`rav1e_*`), linked from `.asm`
        // files in the same crate's build — not external-library FFI.
        let src = "extern {\n    \
                   fn rav1e_avg_8bpc_ssse3(dst: *mut u8);\n    \
                   fn rav1e_avg_8bpc_avx2(dst: *mut u8);\n}";
        assert!(run_in_crate("rav1e", "src/asm/x86/mc.rs", src).is_empty());
    }

    #[test]
    fn allows_in_crate_asm_symbols_hyphenated_crate() {
        // The package name's `-` is normalized to `_` to match Rust symbol
        // identifiers: package `my-codec` owns `my_codec_*` symbols.
        let src = "extern \"C\" {\n    fn my_codec_blend_avx2(dst: *mut u8);\n}";
        assert!(run_in_crate("my-codec", "src/asm.rs", src).is_empty());
    }

    #[test]
    fn flags_external_library_symbols_without_link() {
        // An extern block declaring a foreign library's own symbols (not
        // crate-namespaced) is genuine external FFI and still needs isolation,
        // even without an explicit `#[link]`.
        let src = "extern \"C\" {\n    fn deflate(strm: *mut u8) -> i32;\n}";
        assert_eq!(run_in_crate("rav1e", "src/lib.rs", src).len(), 1);
    }

    #[test]
    fn flags_crate_prefixed_symbols_with_link_attr() {
        // A `#[link(name = ...)]` marker means a real external library boundary,
        // so the crate-namespaced-symbol exemption must not apply.
        let src = "#[link(name = \"rav1e_extern\")]\n\
                   extern \"C\" {\n    fn rav1e_thing(dst: *mut u8);\n}";
        assert_eq!(run_in_crate("rav1e", "src/lib.rs", src).len(), 1);
    }

    #[test]
    fn flags_block_mixing_crate_and_foreign_symbols() {
        // If any declared symbol is not crate-namespaced, the block binds an
        // external library and stays flagged.
        let src = "extern {\n    \
                   fn rav1e_avg_8bpc_avx2(dst: *mut u8);\n    \
                   fn libc_memcpy(dst: *mut u8);\n}";
        assert_eq!(run_in_crate("rav1e", "src/asm/x86/mc.rs", src).len(), 1);
    }

    #[test]
    fn flags_foreign_symbols_unrelated_crate() {
        // `rav1e_*` symbols are not namespaced under the resolved crate, so the
        // exemption does not apply and the block is flagged as before.
        let src = "extern {\n    fn rav1e_avg_8bpc_avx2(dst: *mut u8);\n}";
        assert_eq!(run_in_crate("some-other-crate", "src/lib.rs", src).len(), 1);
    }

    #[test]
    fn allows_sys_crate_binding_block() {
        // A `-sys` crate is a dedicated raw-FFI binding crate; its `extern "C"`
        // block IS the isolation layer.
        let src = "extern \"C\" {\n    fn ZSTD_compress(dst: *mut u8) -> usize;\n}";
        assert!(run_in_crate("zstd-sys", "src/lib.rs", src).is_empty());
    }

    #[test]
    fn allows_cpp_binding_crate_block() {
        // rust-snappy `snappy-cpp/src/lib.rs`: a hand-written C++ binding crate
        // (no `[package].links`, not generated) whose `-cpp` name marks it as a
        // dedicated binding crate.
        let src = "extern \"C\" {\n    \
                   fn snappy_compress(input: *const u8) -> i32;\n    \
                   fn snappy_uncompress(compressed: *const u8) -> i32;\n}";
        assert!(run_in_crate("snappy-cpp", "src/lib.rs", src).is_empty());
    }

    #[test]
    fn allows_links_native_library_crate_block() {
        // A crate declaring `[package].links` is the one crate Cargo permits to
        // bind that native library — the `extern "C"` block is its FFI surface.
        let cargo_toml = "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\nlinks = \"zstd\"\n";
        let src = "extern \"C\" {\n    fn ZSTD_compress(dst: *mut u8) -> usize;\n}";
        assert!(run_in_manifest(cargo_toml, "src/lib.rs", src).is_empty());
    }

    #[test]
    fn flags_unisolated_ffi_in_ordinary_crate() {
        // An ordinary application/library crate (no `links` key, not `-sys`/`-cpp`,
        // not generated) with an un-isolated foreign-library `extern "C"` block is
        // still flagged.
        let src = "extern \"C\" {\n    fn deflate(strm: *mut u8) -> i32;\n}";
        assert_eq!(run_in_crate("my-app", "src/lib.rs", src).len(), 1);
    }
}
