//! rust-bool-param-in-pub-fn backend.
//!
//! Walks `function_item` and `function_signature_item` nodes and reports one
//! diagnostic per parameter whose declared type is exactly `bool`. `Option<bool>`
//! and `&bool` are not flagged — they are not the two-state flag the rule is
//! about.
//!
//! The subject is the public surface. A free function or an inherent-impl method
//! must carry a visibility modifier (`pub` or `pub(crate)`, via
//! [`is_pub_including_restricted`]); a trait member — a `function_signature_item`
//! or a default-bodied `function_item` directly inside a `trait_item` — has no
//! modifier of its own, so it inherits the trait's. A method inside
//! `impl Trait for Type` is therefore out of scope for free: it cannot carry a
//! visibility modifier, and its signature is the trait's to change, not the
//! implementor's.
//!
//! Exempt shapes:
//!
//! - a builder/setter name — `set_*`, `with_*`, `enable_*`, `is_*` — where
//!   `.with_verbose(true)` is the established idiom and the name already says
//!   what the `bool` selects;
//! - the same idiom written without a name prefix: a method whose only argument
//!   is the `bool` and which returns the builder (`Self`, `&mut Self`, or the
//!   `impl`'s own type, as in hyper's
//!   `pub fn title_case_headers(&mut self, enabled: bool) -> &mut Builder`).
//!   `.title_case_headers(true)` reads as the property being assigned, exactly
//!   like `with_*`. A `Result`-returning setter counts too — the `ignore` crate
//!   writes `-> Result<&mut GitignoreBuilder, Error>`;
//! - `extern` functions, whose signature is fixed by the FFI ABI on the other
//!   side;
//! - functions exported to another language, on the function or on its enclosing
//!   `impl` (`#[wasm_bindgen]`, `#[pyfunction]`, `#[pymethods]`, `#[napi]`,
//!   `#[uniffi::export]`) or through a linker symbol (`#[no_mangle]`,
//!   `#[export_name]`) — the foreign caller has no enum to pass;
//! - `fn main` and test code, neither of which has a caller to read the flag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    fn_is_extern, has_outer_attribute_path, has_symbol_export_attribute, has_test_attribute,
    is_in_test_context, is_pub_including_restricted,
};

/// Name prefixes where a `bool` argument is already self-describing at the call
/// site: `config.with_verbose(true)` / `handle.set_visible(false)` read as the
/// property being assigned, so the enum rewrite buys nothing.
const SETTER_PREFIXES: &[&str] = &["set_", "with_", "enable_", "is_"];

/// Attribute paths that export the function to another language's caller, on the
/// function itself or on its enclosing `impl`. That caller passes a native
/// boolean and has no way to name a Rust enum variant, so the signature is not
/// the author's to redesign.
const FOREIGN_EXPORT_ATTRIBUTES: &[&str] = &[
    "wasm_bindgen",
    "pyfunction",
    "pymethods",
    "pyclass",
    "napi",
    "uniffi::export",
];

crate::ast_check! { on ["function_item", "function_signature_item"] prefilter = ["bool"] => |node, source, ctx, diagnostics|
    if ctx.file.path_segments.in_test_dir { return; }
    // `is_in_test_context` reads the ANCESTORS' attributes, so a `#[test]` or
    // `#[cfg(test)]` written on this very function needs its own check.
    if is_in_test_context(node, source) || has_test_attribute(node, source) { return; }
    if !is_public_api_fn(node, source) { return; }

    let Some(name_node) = node.child_by_field_name("name") else { return; };
    let Ok(fn_name) = name_node.utf8_text(source) else { return; };
    // The binary entry point is called by the runtime, not by code a reader has
    // to decipher.
    if fn_name == "main" { return; }
    if SETTER_PREFIXES.iter().any(|prefix| fn_name.starts_with(prefix)) { return; }
    if is_builder_setter(node, source) { return; }
    if fn_is_extern(node, source) { return; }
    if is_exported_to_foreign_caller(node, source) { return; }

    let Some(parameters) = node.child_by_field_name("parameters") else { return; };
    let mut cursor = parameters.walk();
    for parameter in parameters.children(&mut cursor) {
        if parameter.kind() != "parameter" {
            continue;
        }
        // Exactly `bool`: `Option<bool>` carries a third state and `&bool` is a
        // borrow, neither of which is the two-state flag this rule is about.
        let is_bool = parameter
            .child_by_field_name("type")
            .and_then(|ty| ty.utf8_text(source).ok())
            .is_some_and(|text| text.trim() == "bool");
        if !is_bool {
            continue;
        }
        let param_name = parameter
            .child_by_field_name("pattern")
            .and_then(|pattern| pattern.utf8_text(source).ok())
            .unwrap_or("flag");
        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &parameter,
            super::META.id,
            format!(
                "`{param_name}: bool` on public `{fn_name}` reads as `{fn_name}(…, true)` at the call site, \
                 which names nothing. Take a two-variant enum instead \
                 (`enum Recursion {{ Recursive, Flat }}`)."
            ),
            Severity::Error,
        ));
    }
}

/// True when the function is part of an API someone else calls: a trait member
/// inherits the trait's visibility, anything else must carry its own `pub` /
/// `pub(crate)`.
///
/// A method in `impl Trait for Type` falls out here: Rust forbids a visibility
/// modifier on it, so it is never public by this measure — which is the wanted
/// answer, since the trait, not the impl, owns that signature.
fn is_public_api_fn(item: tree_sitter::Node, source: &[u8]) -> bool {
    match enclosing_trait_definition(item) {
        Some(trait_item) => is_pub_including_restricted(trait_item, source),
        None => is_pub_including_restricted(item, source),
    }
}

/// The `trait_item` whose body declares `item` directly, or `None`. Membership is
/// tested on the parent chain `item` → `declaration_list` → `trait_item`, so a
/// function nested inside a default method's body — a different `declaration_list`
/// away — is correctly not a trait member.
fn enclosing_trait_definition<'tree>(item: tree_sitter::Node<'tree>) -> Option<tree_sitter::Node<'tree>> {
    item.parent()
        .filter(|body| body.kind() == "declaration_list")?
        .parent()
        .filter(|owner| owner.kind() == "trait_item")
}

/// True for the prefix-less builder setter: the `bool` is the method's only
/// argument and the method hands the builder back, so the call site reads as an
/// assignment (`builder.title_case_headers(true)`) rather than as a flag passed
/// to an operation.
///
/// The return type is accepted as `Self` or as the enclosing `impl`'s own type,
/// through any number of `&` / lifetime / `mut` qualifiers — hyper writes
/// `-> &mut Builder`, others `-> Self`.
fn is_builder_setter(item: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parameters) = item.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = parameters.walk();
    let argument_count = parameters
        .children(&mut cursor)
        .filter(|child| child.kind() == "parameter")
        .count();
    if argument_count != 1 {
        return false;
    }
    let Some(returned) = item
        .child_by_field_name("return_type")
        .and_then(|node| node.utf8_text(source).ok())
        .map(builder_return_base_name)
    else {
        return false;
    };
    returned == "Self" || Some(returned) == enclosing_impl_type_name(item, source)
}

/// The type a builder setter hands back, seen through a fallible wrapper:
/// `Result<&mut GitignoreBuilder, Error>` answers `"GitignoreBuilder"`, because a
/// setter that validates its argument is the same idiom with an error path.
fn builder_return_base_name(type_text: &str) -> &str {
    let base = owned_type_base_name(type_text);
    if base != "Result" {
        return base;
    }
    let Some((_, arguments)) = type_text.split_once('<') else {
        return base;
    };
    owned_type_base_name(arguments.split(',').next().unwrap_or(arguments))
}

/// The bare type name a return type resolves to: leading `&`, lifetimes and
/// `mut` are stripped, then generic arguments and the `::` path prefix
/// (`&'a mut crate::Builder<T>` → `"Builder"`).
fn owned_type_base_name(type_text: &str) -> &str {
    let mut rest = type_text.trim();
    loop {
        let stripped = rest.trim_start_matches('&').trim_start();
        let stripped = match stripped.strip_prefix("mut ") {
            Some(after) => after.trim_start(),
            None => stripped,
        };
        // A lifetime (`'a`) runs to the next whitespace; drop it and re-check,
        // since `&'a mut T` interleaves all three qualifier kinds.
        let stripped = if stripped.starts_with('\'') {
            let cut = stripped.find(char::is_whitespace).unwrap_or(stripped.len());
            stripped[cut..].trim_start()
        } else {
            stripped
        };
        if stripped == rest {
            break;
        }
        rest = stripped;
    }
    let head = rest.split('<').next().unwrap_or(rest).trim();
    head.rsplit("::").next().unwrap_or(head)
}

/// The bare type name of the `impl` block holding `item`, or `None` outside an
/// `impl`.
fn enclosing_impl_type_name<'a>(item: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    enclosing_impl_block(item)?
        .child_by_field_name("type")?
        .utf8_text(source)
        .ok()
        .map(owned_type_base_name)
}

/// True when the function, or the `impl` block holding it, carries an attribute
/// that exports it to a foreign caller. The `impl` is checked too because the
/// per-method attribute is written once on the block in PyO3 (`#[pymethods]`),
/// wasm-bindgen and napi.
fn is_exported_to_foreign_caller(item: tree_sitter::Node, source: &[u8]) -> bool {
    if has_outer_attribute_path(item, source, FOREIGN_EXPORT_ATTRIBUTES)
        || has_symbol_export_attribute(item, source)
    {
        return true;
    }
    enclosing_impl_block(item).is_some_and(|impl_item| {
        has_outer_attribute_path(impl_item, source, FOREIGN_EXPORT_ATTRIBUTES)
            || has_symbol_export_attribute(impl_item, source)
    })
}

/// The `impl` block holding `item` as a direct member, or `None`.
fn enclosing_impl_block<'tree>(item: tree_sitter::Node<'tree>) -> Option<tree_sitter::Node<'tree>> {
    item.parent()
        .filter(|body| body.kind() == "declaration_list")?
        .parent()
        .filter(|owner| owner.kind() == "impl_item")
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.rs")
    }

    #[test]
    fn flags_bool_param_in_pub_fn() {
        let found = run("pub fn walk(path: &str, recursive: bool) {}");
        assert_eq!(found.len(), 1);
        assert!(found[0].message.contains("recursive"), "{}", found[0].message);
    }

    #[test]
    fn flags_bool_param_in_pub_method() {
        assert_eq!(run("impl W { pub fn render(&self, pretty: bool) {} }").len(), 1);
    }

    #[test]
    fn flags_bool_param_in_pub_crate_fn() {
        assert_eq!(run("pub(crate) fn walk(recursive: bool) {}").len(), 1);
    }

    #[test]
    fn flags_each_bool_param_separately() {
        assert_eq!(run("pub fn f(dry_run: bool, verbose: bool) {}").len(), 2);
    }

    #[test]
    fn flags_trait_method_signature_in_pub_trait() {
        assert_eq!(run("pub trait Fs { fn walk(&self, recursive: bool); }").len(), 1);
    }

    #[test]
    fn flags_default_trait_method_in_pub_trait() {
        assert_eq!(run("pub trait Fs { fn walk(&self, recursive: bool) {} }").len(), 1);
    }

    #[test]
    fn allows_private_fn() {
        assert!(run("fn walk(recursive: bool) {}").is_empty());
    }

    #[test]
    fn allows_method_in_trait_impl() {
        assert!(run("impl Fs for Local { fn walk(&self, recursive: bool) {} }").is_empty());
    }

    #[test]
    fn allows_method_in_private_trait() {
        assert!(run("trait Fs { fn walk(&self, recursive: bool); }").is_empty());
    }

    #[test]
    fn allows_optional_bool_param() {
        assert!(run("pub fn walk(recursive: Option<bool>) {}").is_empty());
    }

    #[test]
    fn allows_bool_reference_param() {
        assert!(run("pub fn walk(recursive: &bool) {}").is_empty());
    }

    #[test]
    fn allows_bool_return_type() {
        assert!(run("pub fn contains(&self, value: u32) -> bool { true }").is_empty());
    }

    #[test]
    fn allows_setter_named_fn() {
        assert!(run("pub fn set_visible(&mut self, visible: bool) {}").is_empty());
    }

    #[test]
    fn allows_builder_with_named_fn() {
        assert!(run("pub fn with_verbose(self, verbose: bool) -> Self { self }").is_empty());
    }

    #[test]
    fn allows_prefixless_builder_setter_returning_self() {
        assert!(run("impl B { pub fn verbose(&mut self, enabled: bool) -> &mut Self { self } }").is_empty());
    }

    #[test]
    fn allows_prefixless_builder_setter_returning_named_type() {
        let src = "impl Builder { pub fn title_case_headers(&mut self, enabled: bool) -> &mut Builder { self } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_fallible_builder_setter() {
        let src = "impl GitignoreBuilder { pub fn case_insensitive(&mut self, yes: bool) -> Result<&mut GitignoreBuilder, Error> { Ok(self) } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_bool_beside_another_argument_even_when_returning_self() {
        let src = "impl B { pub fn set(&mut self, key: &str, enabled: bool) -> &mut Self { self } }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_single_bool_argument_when_nothing_is_returned() {
        assert_eq!(run("impl B { pub fn render(&self, pretty: bool) {} }").len(), 1);
    }

    #[test]
    fn allows_enable_named_fn() {
        assert!(run("pub fn enable_color(&mut self, on: bool) {}").is_empty());
    }

    #[test]
    fn allows_extern_c_fn() {
        assert!(run("pub extern \"C\" fn callback(flag: bool) {}").is_empty());
    }

    #[test]
    fn allows_wasm_bindgen_fn() {
        assert!(run("#[wasm_bindgen]\npub fn render(pretty: bool) {}").is_empty());
    }

    #[test]
    fn allows_wasm_bindgen_fn_with_arguments() {
        assert!(run("#[wasm_bindgen(js_name = render)]\npub fn render(pretty: bool) {}").is_empty());
    }

    #[test]
    fn allows_method_in_pymethods_impl() {
        assert!(run("#[pymethods]\nimpl W { pub fn render(&self, pretty: bool) {} }").is_empty());
    }

    #[test]
    fn allows_no_mangle_fn() {
        assert!(run("#[no_mangle]\npub fn render(pretty: bool) {}").is_empty());
    }

    #[test]
    fn allows_fn_main() {
        assert!(run("pub fn main(flag: bool) {}").is_empty());
    }

    #[test]
    fn allows_test_fn() {
        assert!(run("#[test]\npub fn checks(flag: bool) {}").is_empty());
    }

    #[test]
    fn allows_fn_in_test_module() {
        assert!(run("#[cfg(test)]\nmod tests { pub fn helper(flag: bool) {} }").is_empty());
    }
}
