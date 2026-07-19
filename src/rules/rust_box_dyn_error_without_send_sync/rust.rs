//! rust-box-dyn-error-without-send-sync backend.
//!
//! Walks `generic_type` nodes whose constructor is `Box` and whose
//! sole type argument is a `dyn`-typed trait object referencing the
//! `Error` trait. We then check whether the trait object's bounds
//! include both `Send` and `Sync`. If either is missing, we flag it —
//! unless the bounds carry an explicit non-`'static` lifetime (e.g.
//! `Box<dyn Error + 'a>`), which marks a borrow-scoped error for which
//! the `+ 'static` remediation is inapplicable.
//!
//! The check is text-based on the trait-object substring because
//! tree-sitter-rust models `dyn Trait + Send + Sync` as a single
//! `dynamic_type` whose internal layout is grammar-version
//! dependent — substring matching is robust enough. To avoid false
//! positives we require the `Error` token to be the *primary* trait of
//! the outer `dyn` (`dyn Error ...` or `dyn ...::Error ...`), not merely
//! to appear somewhere inside an inner type's generics (e.g.
//! `dyn Future<Output = Result<_, Self::Error>>`).
//!
//! Test contexts, `fn main`, Cargo build scripts (`build.rs`), and files
//! in a binary crate (one whose nearest `Cargo.toml` declares a `[[bin]]`
//! target or carries `src/main.rs`) are exempt: the error stays
//! single-threaded (the binary entry point and the helpers it calls print
//! to stderr; a build script runs synchronously at compile time), so the
//! `Send + Sync` remediation is structurally inapplicable.
//!
//! A `Box<dyn Error>` that is the self type of an `impl … for Box<dyn Error>`,
//! or that appears anywhere in such an impl's body, is also exempt:
//! `Box<dyn Error>` and `Box<dyn Error + Send + Sync>` are distinct types, so
//! adding the bounds would change *which* type the impl is for. A method in
//! such an impl constructs and returns that same self type, so we exempt the
//! whole body rather than try to tell self-typed positions apart. This keys on
//! the enclosing impl's self type, so a `Box<dyn Error>` in the *body* of an
//! `impl … for ConcreteType` still flags.
//!
//! A `Box<dyn Error>` in the return type or a parameter of a method inside a
//! trait impl (`impl Trait for Type`) is exempt: that signature is dictated by
//! the trait declaration, so adding `+ Send + Sync` would make it no longer
//! match the trait (E0053) — the implementor structurally can't apply the
//! remediation, which belongs on the trait definition instead. The skip is
//! scoped to signature positions: a `Box<dyn Error>` in the method body, in an
//! inherent-impl method, or in a free function still flags.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::{
    is_in_fn_main, is_in_test_context, is_in_trait_impl, is_under_tests_dir,
};

const KINDS: &[&str] = &["generic_type"];

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
        let source_bytes = ctx.source.as_bytes();
        let Some((has_send, has_sync)) = box_dyn_error_missing_bounds(node, source_bytes) else {
            return;
        };
        // `Box<dyn Error>` as the self type of an `impl … for Box<dyn Error>`,
        // or in a method signature/body pinned to that self type, can't gain
        // `Send + Sync` without changing which type the impl is for — the
        // remediation is structurally impossible. Exempt those positions.
        if is_in_box_dyn_error_self_type_impl(node, source_bytes) {
            return;
        }
        // A `Box<dyn Error>` in the return type or a parameter of a trait-impl
        // method has a signature dictated by the trait contract: adding
        // `+ Send + Sync` would stop it matching the trait declaration (E0053),
        // so the implementor can't apply the remediation. Body-local positions
        // in the same method are the author's own choice and still flag.
        if is_in_trait_impl_method_signature(node) {
            return;
        }
        if is_in_test_context(node, source_bytes)
            || is_under_tests_dir(ctx.path)
            || is_in_fn_main(node, source_bytes)
            || crate::rules::path_utils::is_rust_build_script(ctx.path)
            || ctx
                .project
                .nearest_cargo_manifest(ctx.path)
                .is_some_and(|m| m.declares_binary() || m.declares_executable_at(ctx.path))
        {
            return;
        }
        let missing = match (has_send, has_sync) {
            (false, false) => "Send + Sync",
            (false, true) => "Send",
            (true, false) => "Sync",
            (true, true) => unreachable!(),
        };
        diagnostics.push(Diagnostic::at_node(
            std::sync::Arc::clone(&ctx.path_arc),
            &node,
            "rust-box-dyn-error-without-send-sync",
            format!(
                "`Box<dyn Error>` is missing `{missing}` — the error can't \
                 cross thread boundaries. Add `+ Send + Sync + 'static` or \
                 use `anyhow::Error`."
            ),
            Severity::Error,
        ));
    }
}

/// Returns `Some((has_send, has_sync))` when `node` is a `generic_type` of the
/// shape this rule flags — `Box<dyn Error …>` whose boxed trait object's
/// primary trait is `Error`, which lacks at least one of `Send`/`Sync`, and
/// which carries no non-`'static` lifetime bound (a borrow-scoped error can't
/// be `'static`, so the `+ 'static` remediation is impossible). Returns `None`
/// for any other type, including a fully thread-safe `Box<dyn Error + Send +
/// Sync>`.
fn box_dyn_error_missing_bounds(
    node: tree_sitter::Node,
    source_bytes: &[u8],
) -> Option<(bool, bool)> {
    let type_node = node.child_by_field_name("type")?;
    if type_node.utf8_text(source_bytes).ok()? != "Box" {
        return None;
    }
    let args = node.child_by_field_name("type_arguments")?;
    let args_text = args.utf8_text(source_bytes).ok()?;
    // We need a `dyn Error` type argument where `Error` is the primary trait of
    // the outer `dyn` — not `Error` buried inside an inner type's generics
    // (`dyn Future<Output = Result<_, Self::Error>>`).
    if !dyn_primary_trait_is_error(args_text) {
        return None;
    }
    let has_send = args_text.contains("Send");
    let has_sync = args_text.contains("Sync");
    if has_send && has_sync {
        return None;
    }
    if has_non_static_lifetime(args_text) {
        return None;
    }
    Some((has_send, has_sync))
}

/// True when the flagged `Box<dyn Error>` `node` is the self type of an
/// enclosing `impl … for Box<dyn Error>`, or appears anywhere in the body of
/// such an impl.
///
/// `impl Trait for Box<dyn Error>` implements the trait for one specific type;
/// `Box<dyn Error>` and `Box<dyn Error + Send + Sync>` are distinct types, so
/// the self type can't gain bounds. A method in that impl constructs and
/// returns that same self type, so we exempt every `Box<dyn Error>` in the
/// body rather than try to tell self-typed positions apart. We walk to the
/// nearest enclosing `impl_item` and check whether its `type` (self-type) field
/// has the same flagged `Box<dyn Error>` shape. An impl for a concrete type
/// (`impl Sink for MyError`) does not match, so a genuine `Box<dyn Error>`
/// returned there still flags.
fn is_in_box_dyn_error_self_type_impl(node: tree_sitter::Node, source_bytes: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "impl_item" {
            return ancestor.child_by_field_name("type").is_some_and(|self_ty| {
                box_dyn_error_missing_bounds(self_ty, source_bytes).is_some()
            });
        }
        current = ancestor.parent();
    }
    false
}

/// True when the flagged `Box<dyn Error>` `node` sits in the return type or a
/// parameter type of a method whose nearest enclosing impl is a trait impl
/// (`impl Trait for Type`).
///
/// A trait-impl method's signature is fixed by the trait declaration: adding
/// `+ Send + Sync` would make it no longer match the trait (E0053), so the
/// implementor can't apply the remediation — it belongs on the trait
/// definition. The skip is scoped to signature positions via a byte-range
/// check against the method's `return_type`/`parameters` fields: a
/// `Box<dyn Error>` in the method body (e.g. a `let x: Box<dyn Error> = …`
/// local) is the author's own choice and still flags, as does one in an
/// inherent-impl method or a free function. Trait-impl membership is decided by
/// the shared [`is_in_trait_impl`] lever on the nearest enclosing method.
fn is_in_trait_impl_method_signature(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(func) = current {
        if func.kind() == "function_item" {
            if !is_in_trait_impl(func) {
                return false;
            }
            return [
                func.child_by_field_name("return_type"),
                func.child_by_field_name("parameters"),
            ]
            .into_iter()
            .flatten()
            .any(|field| {
                node.start_byte() >= field.start_byte() && node.end_byte() <= field.end_byte()
            });
        }
        current = func.parent();
    }
    false
}

/// True when the outer `dyn` trait object's primary trait is the `Error`
/// trait (`dyn Error ...` or a path `dyn ...::Error ...`), as opposed to
/// `Error` merely appearing inside an inner type's generics
/// (`dyn Future<Output = Result<_, Self::Error>>`).
///
/// We locate the first standalone `dyn` keyword (boundary-checked so
/// `mydyn`/`dynamic` don't match), then read the primary trait path: the
/// text after `dyn`, trimmed, up to the first `<`, `+`, `>`, or whitespace.
fn dyn_primary_trait_is_error(args_text: &str) -> bool {
    let bytes = args_text.as_bytes();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        if &bytes[i..i + 3] == b"dyn" {
            let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
            let after_ok = i + 3 == bytes.len() || !is_ident_char(bytes[i + 3]);
            if before_ok && after_ok {
                let rest = args_text[i + 3..].trim_start();
                let path_end = rest
                    .find(|c: char| c == '<' || c == '+' || c == '>' || c.is_whitespace())
                    .unwrap_or(rest.len());
                let path = &rest[..path_end];
                return path == "Error" || path.ends_with("::Error");
            }
        }
        i += 1;
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Returns true if `args_text` (a type-position substring) carries a
/// lifetime bound whose name is not `static`. A `'` in a type is always a
/// lifetime (type position has no char literals), so we scan for a `'`
/// followed by an identifier and compare the name against `static`.
fn has_non_static_lifetime(args_text: &str) -> bool {
    let bytes = args_text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\'' {
            let name_start = i + 1;
            let mut name_end = name_start;
            while name_end < bytes.len() && is_ident_char(bytes[name_end]) {
                name_end += 1;
            }
            if name_end > name_start && &bytes[name_start..name_end] != b"static" {
                return true;
            }
            i = name_end;
        } else {
            i += 1;
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

    const BIN_CARGO_TOML: &str = r#"
[package]
name = "cargo-insta"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "cargo-insta"
path = "src/main.rs"
"#;

    const LIB_CARGO_TOML: &str = r#"
[package]
name = "mylib"
version = "0.1.0"
edition = "2021"

[lib]
name = "mylib"
path = "src/lib.rs"
"#;

    // Positive assertions run under a library-crate manifest: the binary-crate
    // exemption (`declares_binary`) is manifest-aware, and the bare default
    // test context would otherwise resolve to comply's own (binary) manifest.
    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_cargo(&Check, LIB_CARGO_TOML, source, "src/lib.rs")
    }

    #[test]
    fn flags_bare_box_dyn_error() {
        let source = "fn f() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_box_dyn_error_send_only() {
        let source = "fn f() -> Result<(), Box<dyn std::error::Error + Send>> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_box_dyn_error_send_sync() {
        let source = "fn f() -> Result<(), Box<dyn std::error::Error + Send + Sync>> { Ok(()) }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_box_dyn_error_with_non_static_lifetime() {
        // `Box<dyn Error + 'a>` is a borrow-scoped error: it borrows from the
        // `&'a str` input, so it is intentionally not `'static`. The `+ 'static`
        // remediation is impossible here. (helix command_line.rs:805)
        let source =
            "fn parse(line: &'a str) -> Result<Self, Box<dyn Error + 'a>> { todo!() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_box_dyn_error_static_only() {
        // Only `'static` (no Send + Sync) is still a true positive: the error
        // can be made thread-safe, so the remediation applies.
        let source = "fn f() -> Result<(), Box<dyn std::error::Error + 'static>> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_box_dyn_other_trait() {
        let source = "fn f() -> Box<dyn Iterator<Item = u8>> { todo!() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_concrete_box() {
        let source = "fn f() -> Box<MyError> { todo!() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_dyn_my_error_subclass() {
        // `dyn MyError` should NOT match — only the standalone `Error` token does.
        let source = "fn f() -> Box<dyn MyError> { todo!() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_box_dyn_future_with_self_error_in_generics() {
        // axum from_fn.rs: the Box holds `dyn Future<...>`, not `dyn Error`.
        // `Error` appears only as `Self::Error` inside the Future's generics —
        // it is not the primary trait of the `dyn`, so it must not be flagged.
        // (Failed under the old `contains_word(args_text, "Error")` check.)
        let source = r#"
            impl Service<Request> for Next {
                type Response = Response;
                type Error = Infallible;
                type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_bare_box_dyn_error_no_path() {
        // Bare `dyn Error` (primary trait is the unqualified `Error` token),
        // missing both Send and Sync → still flagged.
        let source = "fn f() -> Result<(), Box<dyn Error>> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_bare_box_dyn_error_send_only_no_path() {
        // Bare `dyn Error + Send` (missing Sync) → still flagged.
        let source = "fn f() -> Result<(), Box<dyn Error + Send>> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_box_dyn_error_in_tokio_test() {
        let source = r#"
            #[tokio::test]
            async fn test() -> Result<(), Box<dyn std::error::Error>> {
                Ok(())
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_box_dyn_error_in_cfg_test_mod() {
        let source = r#"
            #[cfg(test)]
            mod tests {
                fn test_fn() -> Result<(), Box<dyn std::error::Error>> {
                    Ok(())
                }
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_box_dyn_error_in_fn_main() {
        // `fn main() -> Result<(), Box<dyn Error>>` is the binary entry point:
        // the error is printed to stderr, never crossing a thread boundary.
        let source = "fn main() -> Result<(), Box<dyn Error>> { Ok(()) }";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, source, "src/main.rs").is_empty()
        );
    }

    #[test]
    fn allows_box_dyn_error_in_build_script() {
        // Build scripts run single-threaded at compile time (tokei build.rs:21).
        let source =
            "fn generate(out: &str) -> Result<(), Box<dyn std::error::Error>> { Ok(()) }";
        assert!(crate::rules::test_helpers::run_rule(&Check, source, "build.rs").is_empty());
    }

    #[test]
    fn flags_box_dyn_error_in_non_main_fn() {
        // A non-main function in a library crate is still flagged.
        let source =
            "fn helper() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_box_dyn_error_in_binary_crate_helper() {
        // cargo-insta/src/inline.rs: a helper in a synchronous CLI binary
        // crate. If `fn main() -> Result<_, Box<dyn Error>>` is exempt, so is
        // every helper it calls in the same single-threaded binary — the error
        // never crosses a thread boundary.
        let source = "pub fn rewrite() -> Result<usize, Box<dyn Error>> { todo!() }";
        assert!(
            crate::rules::test_helpers::run_rule_with_cargo(
                &Check,
                BIN_CARGO_TOML,
                source,
                "src/inline.rs",
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_box_dyn_error_in_library_crate_helper() {
        // Negative control: the same signature in a library crate (no bin
        // target, no `src/main.rs`) still flags — a library's errors can
        // propagate to a multi-threaded consumer.
        let source = "pub fn rewrite() -> Result<usize, Box<dyn Error>> { todo!() }";
        assert_eq!(
            crate::rules::test_helpers::run_rule_with_cargo(
                &Check,
                LIB_CARGO_TOML,
                source,
                "src/parser.rs",
            )
            .len(),
            1
        );
    }

    #[test]
    fn allows_box_dyn_error_as_impl_self_type() {
        // ripgrep crates/searcher/src/sink.rs:54: `impl SinkError for
        // Box<dyn Error>`. The self type can't gain `+ Send + Sync` without
        // changing which type the impl is for, and the method's return type
        // (line 57) and body (line 58) are pinned to that self type.
        let source = r#"
            impl SinkError for Box<dyn std::error::Error> {
                fn error_message<T: std::fmt::Display>(
                    message: T,
                ) -> Box<dyn std::error::Error> {
                    Box::<dyn std::error::Error>::from(message.to_string())
                }
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_box_dyn_error_return_in_trait_impl_for_concrete_type() {
        // `impl SinkError for MyError { fn boxed() -> Box<dyn Error> }`: even
        // with a concrete self type, `boxed` is a `SinkError` trait method, so
        // its return type is fixed by the trait declaration and can't gain
        // `+ Send + Sync` (E0053). Same shape as the tikv `impl ConfigManager
        // for RaftstoreConfigManager { fn dispatch }` repro.
        let source = r#"
            impl SinkError for MyError {
                fn boxed() -> Box<dyn std::error::Error> { todo!() }
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_box_dyn_error_struct_field() {
        // Negative control: a struct field outside any impl still flags.
        let source = "struct S { e: Box<dyn std::error::Error> }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_box_dyn_error_return_in_trait_impl_method() {
        // tikv coprocessor/config.rs:229: `fn dispatch` implements
        // `ConfigManager::dispatch`, whose return type `Result<(), Box<dyn
        // Error>>` is fixed by the trait. Adding `+ Send + Sync` would stop the
        // method matching the trait declaration (E0053), so the implementor
        // can't apply the remediation.
        let source = r#"
            impl ConfigManager for RaftstoreConfigManager {
                fn dispatch(
                    &mut self,
                    change: ConfigChange,
                ) -> std::result::Result<(), Box<dyn std::error::Error>> {
                    Ok(())
                }
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_box_dyn_error_param_in_trait_impl_method() {
        // A parameter type is equally dictated by the trait declaration.
        let source = r#"
            impl Handler for MyHandler {
                fn handle(&self, e: Box<dyn std::error::Error>) {}
            }
        "#;
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_box_dyn_error_return_in_inherent_impl_method() {
        // Precision guard: an inherent-impl method signature is the author's
        // own — the `Box<dyn Error>` can be made `Send + Sync`, so it flags.
        let source = r#"
            impl SplitConfig {
                fn validate(&self) -> Result<(), Box<dyn std::error::Error>> {
                    Ok(())
                }
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_box_dyn_error_body_local_in_trait_impl_method() {
        // Precision guard: a body-local `Box<dyn Error>` inside a trait-impl
        // method is the author's own choice (not part of the trait-fixed
        // signature), so it still flags.
        let source = r#"
            impl Handler for MyHandler {
                fn handle(&self) {
                    let e: Box<dyn std::error::Error> = todo!();
                }
            }
        "#;
        assert_eq!(run_on(source).len(), 1);
    }
}
