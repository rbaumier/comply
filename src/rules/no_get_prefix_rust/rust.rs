use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_trait_definition, is_in_trait_impl};

crate::ast_check! { on ["function_item"] prefilter = ["get_"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };

    if !name.starts_with("get_") { return; }

    // `get_and_<verb>` (e.g. `get_and_reset`, `get_and_clear`, `get_and_take`)
    // is a compound read-modify-write operation — atomically read the value AND
    // mutate it — mirroring the `fetch_and_*` atomics. Here `get` is the first
    // half of the compound verb, not a dispensable accessor prefix; stripping it
    // yields the nonsensical `and_reset`. Matched on the `and` segment, so
    // `get_android` (bare name `android`) is still flagged.
    if name[4..].starts_with("and_") { return; }

    // A method inside `impl Trait for Type` takes its name verbatim from the
    // trait declaration; the implementor cannot rename it. Inherent impls
    // (`impl Type`) and free functions keep being flagged — the author owns
    // the name there.
    if is_in_trait_impl(node) { return; }

    // A `get_`-prefixed method declared inside a `trait { … }` definition (a
    // declaration or a default method) is the trait's public API contract: the
    // author cannot rename it without a breaking change, and implementors inherit
    // the name verbatim.
    if is_in_trait_definition(node) { return; }

    // Stripping `get_` from e.g. `get_ref`/`get_mut` would yield a Rust
    // reserved keyword, which is not a legal method name. The suggested rename
    // is impossible, so these accessors are forced to keep the prefix.
    if is_rust_keyword(&name[4..]) { return; }

    // Stripping `get_` from e.g. `get_u8`/`get_i32`/`get_bool` would yield a Rust
    // primitive type name; `buf.u8()` collides with the type name and is
    // confusing, so the prefix is the only sensible name.
    if is_rust_primitive_type_name(&name[4..]) { return; }

    // A method named exactly like one of the standard library's `get_`-prefixed
    // indexed/unsafe accessors mirrors that established std API by name; a custom
    // type implementing the same contract must keep the exact name, and stripping
    // `get_` would break the std-mirroring convention (e.g. `unchecked()` no
    // longer signals it is the unsafe sibling of the checked `get()`).
    if is_std_mirrored_accessor(&name[4..]) { return; }

    if !has_self_param(node, source) { return; }

    // A method that takes a key/index argument beyond `self` is a keyed lookup
    // (`HashMap::get(&self, k)`, `slice::get(&self, index)`), not a field
    // accessor. The C-GETTER convention targets parameterless accessors only;
    // `get`/`get_` is the idiomatic name for a lookup, so do not flag it.
    if takes_non_self_param(node) { return; }

    let ret = match node.child_by_field_name("return_type") {
        Some(r) => r,
        None => return,
    };
    let Ok(ret_text) = ret.utf8_text(source) else { return };

    if ret_text.contains("Result") || ret_text.contains("Option") {
        return;
    }

    if sibling_method_named(node, &name[4..], source) { return; }

    // A `get_$X` method paired with a `set_$X` method in the same impl block is
    // an accessor pair, not an infallible C-GETTER. The pair follows the
    // get/set convention (mandated verbatim by scripting-engine property
    // registration APIs such as Rhai's `register_get_set`/`with_get_set`, which
    // bind `Type::get_x`/`Type::set_x` by name), so the `get_` prefix is part of
    // the contract and renaming would desync the pair.
    if sibling_method_named(node, &format!("set_{}", &name[4..]), source) { return; }

    // A `get_`-prefixed method whose body is a thin safe wrapper delegating to a
    // foreign function (an `unsafe` call qualified by an FFI/`-sys` module path,
    // e.g. `unsafe { zstd_sys::ZSTD_getBlockSize(..) }`) mirrors the wrapped C
    // API name (`ZSTD_getBlockSize` → `get_block_size`); the name is dictated by
    // the foreign library, not chosen as an idiomatic Rust getter, so RFC 344's
    // "drop the `get_`" rename does not apply.
    if wraps_foreign_call(node, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &name_node,
        super::META.id,
        format!("Accessor `{name}` uses `get_` prefix — rename to `{}`. Reserve `get` for fallible operations.", &name[4..]),
        Severity::Warning,
    ));
}

/// True when a method named `bare_name` is defined alongside this
/// `get_`-prefixed accessor in the same impl block. Rust permits only one
/// method per name per impl, so when `foo` already exists (e.g. a
/// builder-pattern setter that consumes `self`), the getter is forced to
/// be `get_foo` — the prefix is the only legal disambiguation, not a smell.
fn sibling_method_named(func: tree_sitter::Node, bare_name: &str, source: &[u8]) -> bool {
    let Some(body) = func.parent() else { return false };
    if body.kind() != "declaration_list" {
        return false;
    }
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "function_item"
            && let Some(n) = child.child_by_field_name("name")
            && n.utf8_text(source) == Ok(bare_name)
        {
            return true;
        }
    }
    false
}

/// True when the method body contains an `unsafe` block that delegates to a
/// foreign function — a `call_expression` whose callee path carries an FFI
/// module marker segment (`ffi`, `sys`, or a `*_sys`/`*_ffi` crate such as
/// `zstd_sys`). Such a `get_` method is a thin safe wrapper whose name mirrors
/// the wrapped C API (e.g. `ZSTD_getBlockSize` → `get_block_size`), so the
/// prefix is dictated by the foreign library rather than chosen as a getter.
///
/// The marker is structural: it requires a path-qualified call (`scoped_identifier`)
/// inside an `unsafe_block`. An ordinary field getter has no call; an `unsafe`
/// block doing a raw deref (`unsafe { *self.ptr }`) or an unqualified local call
/// (`unsafe { helper() }`) has no FFI-marked path segment, so neither matches.
fn wraps_foreign_call(func: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = func.child_by_field_name("body") else {
        return false;
    };
    contains_foreign_unsafe_call(body, source, false)
}

/// Recursively scan `node` for a `call_expression` whose callee is an
/// FFI-marked scoped path, requiring the call to sit inside an `unsafe_block`.
/// `in_unsafe` tracks whether the current subtree is already under one.
fn contains_foreign_unsafe_call(node: tree_sitter::Node, source: &[u8], in_unsafe: bool) -> bool {
    let in_unsafe = in_unsafe || node.kind() == "unsafe_block";

    if in_unsafe
        && node.kind() == "call_expression"
        && let Some(callee) = node.child_by_field_name("function")
        && scoped_path_has_ffi_marker(callee, source)
    {
        return true;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if contains_foreign_unsafe_call(child, source, in_unsafe) {
            return true;
        }
    }
    false
}

/// True when `callee` is a `scoped_identifier` (possibly nested, e.g.
/// `libfoo_sys::ffi::bar`) any of whose `identifier` segments is an FFI module
/// marker: exactly `ffi`/`sys`, or ending in `_sys`/`_ffi` (the `-sys`-crate
/// convention, `zstd_sys`). A bare `identifier` callee (unqualified local call)
/// is not a foreign delegation.
fn scoped_path_has_ffi_marker(callee: tree_sitter::Node, source: &[u8]) -> bool {
    if callee.kind() != "scoped_identifier" {
        return false;
    }
    scoped_segments(callee).into_iter().any(|seg_node| {
        seg_node
            .utf8_text(source)
            .is_ok_and(is_ffi_module_marker)
    })
}

/// Yields the `identifier` segments of a (possibly nested) `scoped_identifier`
/// path. The grammar nests left-recursively: `a::b::c` is
/// `scoped_identifier(path: scoped_identifier(path: a, name: b), name: c)`, so
/// the `name` of each level plus the innermost `path` identifier are the segments.
/// Non-identifier roots (`crate`, `super`, `self`) are skipped — they cannot be
/// FFI markers.
fn scoped_segments(node: tree_sitter::Node) -> Vec<tree_sitter::Node> {
    let mut segments = Vec::new();
    let mut current = Some(node);
    while let Some(n) = current {
        if let Some(name) = n.child_by_field_name("name")
            && name.kind() == "identifier"
        {
            segments.push(name);
        }
        match n.child_by_field_name("path") {
            Some(p) if p.kind() == "scoped_identifier" => current = Some(p),
            Some(p) => {
                if p.kind() == "identifier" {
                    segments.push(p);
                }
                current = None;
            }
            None => current = None,
        }
    }
    segments
}

/// True when a path segment names a foreign-function-interface module: the
/// conventional `ffi`/`sys` module names, or a `-sys` crate (`*_sys`) / an
/// `*_ffi` module.
fn is_ffi_module_marker(seg: &str) -> bool {
    seg == "ffi" || seg == "sys" || seg.ends_with("_sys") || seg.ends_with("_ffi")
}

/// Canonical set of Rust reserved keywords (strict + reserved-for-future),
/// excluding contextual keywords that are valid identifiers (`union`, `dyn`,
/// `'static`). A method cannot be named with any of these, so an accessor whose
/// bare name would collide with one is exempt from the rename.
fn is_rust_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "break"
            | "const"
            | "continue"
            | "crate"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "async"
            | "await"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "macro"
            | "override"
            | "priv"
            | "try"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
    )
}

/// True when `name` is a Rust primitive type name (`u8`, `i32`, `bool`, `str`,
/// …). Stripping `get_` from an accessor like `get_u8` would suggest `u8()`,
/// which collides with the type name and reads confusingly, so such accessors
/// are exempt from the rename.
fn is_rust_primitive_type_name(name: &str) -> bool {
    matches!(
        name,
        "u8" | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
            | "bool"
            | "char"
            | "str"
    )
}

/// True when the bare name (the part after `get_`) matches a standard library
/// `get_`-prefixed accessor that legitimately keeps the prefix. These names mirror
/// std's indexed/fallible/unsafe accessor API, an established Rust idiom distinct
/// from a `get_field()` getter:
///   - `slice::get_unchecked` / `get_unchecked_mut` — unsafe bounds-unchecked indexing
///   - `slice::get_disjoint_mut` — multiple disjoint mutable indices
///   - `Option::get_or_insert` / `get_or_insert_with` / `get_or_insert_default` —
///     read-or-initialize accessors
/// A custom type implementing the same contract must reuse these exact names, so
/// the `get_` prefix is part of the convention, not a dispensable accessor prefix.
/// (`get_mut` is already exempt via the reserved-keyword guard, `mut`.)
fn is_std_mirrored_accessor(bare_name: &str) -> bool {
    matches!(
        bare_name,
        "unchecked"
            | "unchecked_mut"
            | "disjoint_mut"
            | "or_insert"
            | "or_insert_with"
            | "or_insert_default"
    )
}

fn has_self_param(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(params) = node.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    for child in params.children(&mut cursor) {
        if child.kind() == "self_parameter" {
            return true;
        }
        if let Ok(text) = child.utf8_text(source)
            && text.contains("self") {
                return true;
            }
    }
    false
}

/// True when the method declares a parameter other than its receiver. The
/// `parameters` node's named children are `self_parameter` and `parameter`
/// (typed params); anonymous tokens like `(`, `)`, `,` are excluded by walking
/// `named_children`. Any named child that is not the `self_parameter` is a real
/// argument, marking the method as a keyed lookup rather than a field accessor.
fn takes_non_self_param(node: tree_sitter::Node) -> bool {
    let Some(params) = node.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    params
        .named_children(&mut cursor)
        .any(|child| child.kind() != "self_parameter")
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
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_simple_getter() {
        let src = "impl Foo {\n    fn get_name(&self) -> &str { &self.name }\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("name"));
    }

    #[test]
    fn allows_result_return() {
        let src = "impl Foo {\n    fn get_value(&self) -> Result<i32, Error> { Ok(1) }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_option_return() {
        let src = "impl Foo {\n    fn get_value(&self) -> Option<i32> { Some(1) }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_no_self() {
        let src = "fn get_default_config() -> Config { Config {} }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_get_prefix_when_sibling_setter_exists_issue_1000() {
        let src = "impl DateTimeRound {\n\
            pub fn smallest(mut self, unit: Unit) -> DateTimeRound { self }\n\
            pub fn mode(mut self, mode: RoundMode) -> DateTimeRound { self }\n\
            pub fn increment(mut self, increment: i64) -> DateTimeRound { self }\n\
            pub(crate) fn get_smallest(&self) -> Unit { self.smallest }\n\
            pub(crate) fn get_mode(&self) -> RoundMode { self.mode }\n\
            pub(crate) fn get_increment(&self) -> i64 { self.increment }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_get_prefix_without_sibling() {
        // get_count with no sibling `count` method — the prefix is gratuitous.
        let src = "impl Foo {\n    fn get_count(&self) -> i64 { self.count }\n    fn other(&self) -> i64 { 0 }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_non_get_prefix() {
        let src = "impl Foo {\n    fn name(&self) -> &str { &self.name }\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_keyword_suffix_get_ref_get_mut_issue_1407() {
        // Stripping `get_` yields `ref`/`mut`, which are Rust keywords — the
        // suggested rename is not a legal method name.
        let src = "impl Throttle {\n\
            pub fn get_ref(&self) -> &T { &self.inner }\n\
            pub fn get_mut(&mut self) -> &mut T { &mut self.inner }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_get_prefix_suggests_bare_name() {
        let src = "impl Foo {\n    fn get_name(&self) -> &str { &self.name }\n}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("rename to `name`"), "{:?}", diags);
    }

    #[test]
    fn allows_get_prefix_in_trait_impl_issue_1330() {
        // The method name is dictated by the external trait — the implementor
        // cannot rename it.
        let src = "impl Scroller for Widget {\n\
            fn get_scroller_mut(&mut self) -> &mut Core { &mut self.scroller }\n\
            fn get_scroller(&self) -> &Core { &self.scroller }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_get_prefix_in_inherent_impl_issue_1330() {
        // `impl Widget` (no trait) — the author chose the name and can rename it.
        let src = "impl Widget {\n    fn get_id(&self) -> u32 { self.id }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_get_prefix_free_function_issue_1330() {
        // A free function with `&self` (e.g. a closure-like accessor) is not in
        // any impl — still flagged.
        let src = "impl Widget {\n    fn get_id(&self) -> u32 { self.id }\n    fn unrelated(&self) -> u32 { 0 }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_keyed_lookup_with_param_issue_3942() {
        // `get_version(&self, key)` is a keyed lookup, not a field accessor — it
        // takes an argument beyond `self`, so `get_`/`get` is the idiomatic name.
        let src = "impl MarkerEnvironment {\n\
            pub fn get_version(&self, key: CanonicalMarkerValueVersion) -> &Version { todo!() }\n\
            pub fn get_string(&self, key: CanonicalMarkerValueString) -> &str { todo!() }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_parameterless_getter_when_keyed_sibling_exists_issue_3942() {
        // A parameterless `get_name(&self)` is still a field accessor and must
        // flag even though the guard exempts keyed lookups.
        let src = "impl Foo {\n    pub fn get_name(&self) -> &str { &self.name }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_get_prefix_in_trait_definition_with_default_body_issue_4507() {
        // A `get_`-prefixed default method in a trait definition is the trait's
        // public API contract — the author cannot rename it. Covers `get_u16_le`
        // too, whose bare name is not a primitive type name.
        let src = "pub trait Buf {\n\
            fn get_u8(&mut self) -> u8 { 0 }\n\
            fn get_u16_le(&mut self) -> u16 { 0 }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_get_prefix_in_trait_declaration_issue_4507() {
        // A `get_`-prefixed declaration (no body) in a trait definition is part of
        // the contract and never flagged. (A body-less signature is a
        // `function_signature_item`, outside the rule's `function_item` filter.)
        let src = "pub trait Buf {\n    fn get_i32(&mut self) -> i32;\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_get_prefix_primitive_name_in_inherent_impl_issue_4507() {
        // `get_u8` strips to `u8`, a primitive type name — `foo.u8()` collides
        // with the type and reads confusingly, so the prefix stays even outside a
        // trait.
        let src = "impl Foo {\n    fn get_u8(&self) -> u8 { 0 }\n}";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_get_prefix_non_primitive_name_in_inherent_impl_issue_4507() {
        // `get_name` strips to `name`, not a primitive type name — the author
        // owns the name and can rename to `name()`.
        let src = "impl Foo {\n    fn get_name(&self) -> String { String::new() }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_get_set_accessor_pair_issue_4816() {
        // `get_x` paired with `set_x` is a get/set accessor pair, the naming
        // mandated by Rhai's `with_get_set("x", Self::get_x, Self::set_x)`
        // property registration — renaming `get_x` to `x` would desync the pair.
        let src = "impl Vec3 {\n\
            fn get_x(&mut self) -> INT { self.x }\n\
            fn set_x(&mut self, x: INT) { self.x = x }\n\
            fn get_y(&mut self) -> INT { self.y }\n\
            fn set_y(&mut self, y: INT) { self.y = y }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_get_prefix_without_set_sibling_issue_4816() {
        // A `get_x` with no `set_x` counterpart is an ordinary getter, not an
        // accessor pair — still flagged.
        let src = "impl Vec3 {\n\
            fn get_x(&mut self) -> INT { self.x }\n\
            fn set_y(&mut self, y: INT) { self.y = y }\n\
        }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_get_prefix_non_primitive_return_type_in_inherent_impl_issue_4507() {
        // Only the method name after `get_` (`count`), not the return type
        // (`usize`), matters for the primitive-name guard — `count` is not a
        // primitive type name, so this still flags.
        let src = "impl Foo {\n    fn get_count(&self) -> usize { 0 }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_get_and_reset_compound_operation_issue_5002() {
        // `get_and_reset_*` is a compound read-modify-write op (read AND reset),
        // not a pure accessor — `get` is part of the compound verb.
        let src = "impl RawMetrics {\n\
            fn get_and_reset_local_max_idle_duration(&self) -> Duration { todo!() }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_get_unchecked_std_mirrored_accessor_issue_5049() {
        // `get_unchecked`/`get_unchecked_mut` mirror the std slice unsafe-accessor
        // API (`<[T]>::get_unchecked`); a custom type must keep the exact name, so
        // they are exempt even in an inherent impl with no extra param.
        let src = "impl Slot {\n\
            pub unsafe fn get_unchecked(&self) -> &T { todo!() }\n\
            pub unsafe fn get_unchecked_mut(&mut self) -> &mut T { todo!() }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_real_getter_alongside_std_mirrored_accessor_issue_5049() {
        // A plain `get_name` field accessor is not a std-mirrored accessor and
        // must still flag.
        let src = "impl Slot {\n    fn get_name(&self) -> &str { &self.name }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ffi_wrapper_sys_crate_call_issue_5313() {
        // `get_block_size` is a thin safe wrapper over `zstd_sys::ZSTD_getBlockSize`
        // — the name mirrors the wrapped C API, dictated by the foreign library.
        let src = "impl Foo {\n\
            pub fn get_block_size(&self) -> usize {\n\
                unsafe { zstd_sys::ZSTD_getBlockSize(self.0.as_ptr()) }\n\
            }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn allows_ffi_wrapper_nested_ffi_path_issue_5313() {
        // A nested FFI path (`libfoo_sys::ffi::get_v`) inside an `unsafe` block,
        // bound via `let`, is still a foreign delegation.
        let src = "impl Foo {\n\
            fn get_v(&self) -> u32 { let r = unsafe { libfoo_sys::ffi::get_v(self.p) }; r }\n\
        }";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn flags_ordinary_field_getter_not_ffi_issue_5313() {
        // An ordinary field getter has no foreign call — still flagged.
        let src = "impl Foo {\n    fn get_name(&self) -> &str { &self.name }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unsafe_block_without_foreign_call_issue_5313() {
        // An `unsafe` block doing a raw deref (no FFI-marked call) is not a
        // foreign delegation — the getter is still flagged.
        let src = "impl Foo {\n    fn get_val(&self) -> u32 { unsafe { *self.ptr } }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_unsafe_local_call_not_ffi_path_issue_5313() {
        // An unqualified local call inside `unsafe` (`helper()`) carries no FFI
        // module marker — still flagged.
        let src = "impl Foo {\n    fn get_val(&self) -> u32 { unsafe { helper() } }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_non_ffi_scoped_call_inside_unsafe_issue_5313() {
        // A scoped call whose path has no `ffi`/`sys`/`*_sys` segment
        // (`crate::config::read()`) is not a foreign delegation — still flagged.
        let src = "impl Foo {\n    fn get_val(&self) -> u32 { unsafe { crate::config::read() } }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_foreign_call_outside_unsafe_issue_5313() {
        // A `sys`-pathed call NOT wrapped in `unsafe` is not the FFI safe-wrapper
        // shape (FFI calls are unsafe); the getter is still flagged.
        let src = "impl Foo {\n    fn get_val(&self) -> u32 { sys::read() }\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_get_android_substring_not_segment_issue_5002() {
        // `get_android` strips to `android`, which does not start with the `and_`
        // segment — it is a plain accessor and must still flag.
        let src = "impl Phone {\n    fn get_android(&self) -> &Os { &self.android }\n}";
        assert_eq!(run(src).len(), 1);
    }
}
