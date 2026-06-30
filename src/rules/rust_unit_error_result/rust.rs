//! rust-unit-error-result backend.
//!
//! Walks every type expression and flags `Result<_, ()>` patterns.
//! We match on the AST, not on text, so it catches the type wherever
//! it appears: function return types, struct fields, type aliases,
//! generic bounds, etc.
//!
//! Test-context exception: a `Result<_, ()>` inside a test context
//! (`#[test]` / `#[cfg(test)]` fn, mod, or impl) is exempt. Mock test
//! types idiomatically set `type Error = ()` because the test never
//! exercises the error path.
//!
//! Clippy-allow exception: a `Result<_, ()>` under an enclosing scope that
//! carries `#[allow(clippy::result_unit_err)]` or
//! `#[expect(clippy::result_unit_err)]` (on the function / mod / impl, or the
//! crate root) is exempt — the author has formally dismissed the equivalent
//! clippy lint, so comply honors that suppression.
//!
//! Axum/tower exception: in that ecosystem `()` deliberately implements
//! `IntoResponse` (an empty `200 OK`), so `Result<_, ()>` is idiomatic for
//! handlers and extractors. We exempt the structurally-detectable cases:
//! an `impl IntoResponse` ok-type, a `#[debug_handler]` handler, and a
//! `FromRequest`/`FromRequestParts` extractor whose `type Rejection = ()`.
//!
//! Trait-contract exception: `Result<(), ()>` (BOTH params `()`) in a trait
//! method signature — a trait definition or a trait impl — is a deliberate
//! binary success/failure signal in transport abstractions (e.g. gRPC, where
//! the error detail travels out-of-band via Status trailers). It is an API
//! contract every impl must conform to, not a discarded error. `Result<Value,
//! ()>` (a real value alongside a discarded error) stays flagged everywhere,
//! and `Result<(), ()>` in a free or inherent function stays flagged too.
//!
//! Borrowed-narrowing-accessor exception: a `Result<&T, ()>` that is the return
//! type of a `to_`/`as_`/`try_` function is a type-narrowing accessor — it
//! answers "is `self` interpretable as this borrowed view, and if so what is
//! it?". The `Err(())` means "wrong variant"; the failure is a closed
//! single-element set with no error detail to carry, so `()` is correct. This
//! is `Option<&T>` semantics expressed with `Result` for API consistency. The
//! ok-type must be a reference (`&str`, `&[u8]`, `&T`) and the `Result` must sit
//! in the function's return type — an owned ok-type or a non-accessor name
//! stays flagged.
//!
//! Logos-callback exception: a `Result<T, ()>` that is the return type of a
//! function taking `&mut Lexer<…>` as a parameter is a [logos] lexer callback.
//! Logos invokes such callbacks with `&mut Lexer<Token>` and interprets
//! `Err(())` as "reject this token match" — the `()` error type is mandated by
//! the library's callback contract, not the author's choice. The `&mut Lexer<…>`
//! parameter is the structural signature of the callback API; a function without
//! it stays flagged.
//!
//! [logos]: https://github.com/maciejhirsz/logos
//!
//! FromStr-trait exception: a `Result<T, ()>` that is the return type of a
//! function inside an `impl <…::>FromStr for T` block is mandated by the std
//! `FromStr` trait, whose signature is `fn from_str(&str) -> Result<Self,
//! Self::Err>`. The trait fixes the `Result` return — `Option` is not an
//! available alternative — and when parsing is binary `type Err = ()` is the
//! idiomatic no-detail error type. The enclosing impl's trait must name
//! `FromStr` exactly or via a `::FromStr` path suffix (`std::str::FromStr`,
//! `core::str::FromStr`); a user trait such as `MyFromStr` is not matched.
//!
//! Local-`Result`-alias exception: when the file declares its own
//! `type Result<…> = …` alias (e.g. `type Result<'a, T> =
//! core::result::Result<T, Box<Error<'a>>>`), the alias can reorder std's
//! `Result<T, E>` parameters so the `()` we match sits in the *success*
//! position, not the error position. The positional "second arg is the error"
//! check no longer holds for `Result<…>` usages in that file, so the rule does
//! not fire there. A genuine std `Result<_, ()>` in a file with no such alias
//! still flags.
//!
//! Private-type-alias exception: a `Result<_, ()>` that is the right-hand side
//! of a module-private `type Name<…> = Result<_, ()>;` alias — a `type_item`
//! carrying no visibility modifier — is exempt. Such a named, private alias is
//! the nom/winnow parser-combinator idiom: `Err(())` means "this input did not
//! match; try another combinator", and the error detail is collected
//! out-of-band by the top-level parser. The private alias is a deliberate,
//! auditable author decision with no consumer to mislead. A public alias
//! (`pub` / `pub(crate)` / `pub(super)` — any visibility modifier present) and
//! a direct function-return `Result<_, ()>` still flag.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{
    enclosing_fn, file_has_local_result_alias, is_in_test_context, is_in_trait_definition,
    is_in_trait_impl, is_suppressed_by_clippy_allow, result_error_type, result_ok_type,
};
use tree_sitter::Node;

crate::ast_check! { on ["generic_type"] => |node, source, ctx, diagnostics|
    let Some(err_type) = result_error_type(node, source) else {
        return;
    };
    if err_type.kind() != "unit_type" {
        return;
    }
    // A file-local `Result` alias may reorder T/E; see the module docs and
    // `file_has_local_result_alias`.
    if file_has_local_result_alias(node, source) {
        return;
    }
    // A module-private `type Name<…> = Result<_, ()>;` alias is the nom/winnow
    // parser-combinator idiom; see the module docs and `is_private_type_alias_rhs`.
    if is_private_type_alias_rhs(node) {
        return;
    }
    if is_in_test_context(node, source) {
        return;
    }
    if is_suppressed_by_clippy_allow(node, &["result_unit_err"], source) {
        return;
    }
    if is_axum_unit_response(node, source) {
        return;
    }
    // gRPC/transport trait interfaces use `Result<(), ()>` as a deliberate binary
    // success/failure signal — error detail travels out-of-band (e.g. gRPC Status
    // trailers), not through the Rust error type. When BOTH params are `()` and the
    // type is a trait method signature (definition or impl), it is an API contract
    // every impl must conform to, not a discarded error detail.
    let ok_type_is_unit = result_ok_type(node, source).is_some_and(|t| t.kind() == "unit_type");
    if ok_type_is_unit && (is_in_trait_impl(node) || is_in_trait_definition(node)) {
        return;
    }
    if is_borrowed_narrowing_accessor(node, source) {
        return;
    }
    if is_logos_lexer_callback(node, source) {
        return;
    }
    if is_in_fromstr_impl(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "rust-unit-error-result".into(),
        message: "`Result<_, ()>` discards every error detail. Define a \
                  real error type, or return `Option<T>` if absence is the \
                  only failure mode."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

/// True when this `Result<_, ()>` `generic_type` is the right-hand side of a
/// module-private type alias (`type Name<…> = Result<_, ()>;` with no visibility
/// modifier) — a named, auditable parser-combinator type (nom/winnow style)
/// where `Err(())` is the conventional "input did not match" rejection signal.
///
/// The match is pure AST position + visibility: the `generic_type` must be the
/// alias's declared `type` (its RHS itself, not a `Result<_, ()>` nested as a
/// generic argument deeper on the RHS), the `type_item` must be a free-standing
/// module-scope alias (not an associated type in an `impl`/`trait` body), and it
/// must carry no `visibility_modifier`. Any visibility modifier (`pub`,
/// `pub(crate)`, `pub(super)`) makes the alias non-exempt, and a direct
/// function-return `Result<_, ()>` is not a `type_item` RHS so it stays flagged.
fn is_private_type_alias_rhs(node: Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "type_item" {
        return false;
    }
    // The `generic_type` must be the alias's declared RHS, not nested deeper.
    if parent.child_by_field_name("type") != Some(node) {
        return false;
    }
    // An associated type in an `impl`/`trait` body is also a `type_item` that
    // never carries a visibility modifier, so the visibility check below cannot
    // tell it apart from a private module alias. It is a trait-contract member,
    // not a parser-combinator alias, so it must keep flagging.
    if type_item_is_associated(parent) {
        return false;
    }
    // No visibility modifier at all → module-private alias.
    let mut cursor = parent.walk();
    !parent
        .children(&mut cursor)
        .any(|child| child.kind() == "visibility_modifier")
}

/// True when `type_item` is an associated type inside an `impl`/`trait` body —
/// its enclosing `declaration_list` belongs to an `impl_item`/`trait_item` —
/// rather than a free-standing module-scope (or `mod`-scoped) type alias.
fn type_item_is_associated(type_item: Node) -> bool {
    type_item
        .parent()
        .filter(|p| p.kind() == "declaration_list")
        .and_then(|list| list.parent())
        .is_some_and(|owner| matches!(owner.kind(), "impl_item" | "trait_item"))
}

/// True when this `Result<&T, ()>` is the return type of a `to_`/`as_`/`try_`
/// accessor — a type-narrowing accessor returning a borrowed view of `self`.
///
/// The ok-type must be a `reference_type` (`&str`, `&[u8]`, `&T`), and the
/// `Result` must sit inside the enclosing function's `return_type`, not its
/// body, parameters, a struct field, a type alias, or a closure.
fn is_borrowed_narrowing_accessor(node: Node, source: &[u8]) -> bool {
    // OK type must be a reference (`&str`, `&[u8]`, `&T`) — a borrowed view.
    if result_ok_type(node, source).map(|t| t.kind()) != Some("reference_type") {
        return false;
    }
    // The Result must be the RETURN TYPE of a `to_`/`as_`/`try_` function.
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        match parent.kind() {
            "function_item" => {
                // node must sit inside the function's `return_type`, not its body/params
                let Some(ret) = parent.child_by_field_name("return_type") else {
                    return false;
                };
                if node.start_byte() < ret.start_byte() || node.end_byte() > ret.end_byte() {
                    return false;
                }
                let Some(name) = parent
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                else {
                    return false;
                };
                return name.starts_with("to_")
                    || name.starts_with("as_")
                    || name.starts_with("try_");
            }
            // not inside a function return type (struct field, type alias, closure, etc.)
            "struct_item" | "enum_item" | "type_item" | "closure_expression" | "impl_item"
            | "trait_item" => return false,
            _ => {}
        }
        cur = parent;
    }
    false
}

/// True when this `Result<T, ()>` is the return type of a logos lexer callback —
/// a function that takes `&mut Lexer<…>` as a parameter.
///
/// Logos invokes callbacks with `&mut Lexer<Token>` and reads `Err(())` as
/// "reject this token match", so the `()` error is mandated by the library's
/// callback contract. The `Result` must sit in the enclosing function's
/// `return_type` and that function must have a `&mut Lexer<…>` parameter — a
/// `Result<_, ()>` elsewhere (struct field, type alias, or a function without
/// the lexer parameter) stays flagged.
fn is_logos_lexer_callback(node: Node, source: &[u8]) -> bool {
    let Some(func) = enclosing_fn(node) else {
        return false;
    };
    // The Result must be the function's RETURN TYPE, not its body or parameters.
    let Some(ret) = func.child_by_field_name("return_type") else {
        return false;
    };
    if node.start_byte() < ret.start_byte() || node.end_byte() > ret.end_byte() {
        return false;
    }
    let Some(params) = func.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    params
        .named_children(&mut cursor)
        .filter(|p| p.kind() == "parameter")
        .filter_map(|p| p.child_by_field_name("type"))
        .any(|ty| type_is_mut_ref_to_lexer(ty, source))
}

/// True when `ty` is `&mut Lexer<…>` — a mutable reference to a `Lexer`
/// generic type. The referent must be a `generic_type` whose base path ends in
/// the `Lexer` identifier (so `logos::Lexer<'s, Token>` matches as well as the
/// bare `Lexer<Token>`).
fn type_is_mut_ref_to_lexer(ty: Node, source: &[u8]) -> bool {
    if ty.kind() != "reference_type" {
        return false;
    }
    // `&mut` is exposed as an anonymous `mutable_specifier` child, not a field.
    let mut ty_cursor = ty.walk();
    if !ty
        .children(&mut ty_cursor)
        .any(|c| c.kind() == "mutable_specifier")
    {
        return false;
    }
    let Some(referent) = ty.child_by_field_name("type") else {
        return false;
    };
    if referent.kind() != "generic_type" {
        return false;
    }
    let Some(base) = referent.child_by_field_name("type") else {
        return false;
    };
    // `Lexer` (type_identifier) or `logos::Lexer` (scoped_type_identifier).
    base.utf8_text(source)
        .is_ok_and(|t| t == "Lexer" || t.ends_with("::Lexer"))
}

/// True when this `Result<T, ()>` is the return type of a function inside an
/// `impl <…::>FromStr for T` block — the std `FromStr` trait, whose `from_str`
/// signature is `fn(&str) -> Result<Self, Self::Err>`.
///
/// `FromStr` mandates a `Result` return, so `Option<T>` is not an available
/// alternative; when parsing is binary, `type Err = ()` is the idiomatic
/// no-detail error type. The `Result` must sit in the enclosing function's
/// `return_type` — a `Result<_, ()>` in the body (a let binding, a closure)
/// stays flagged. The enclosing impl's `trait` field must name `FromStr`
/// exactly or via a `::FromStr` path suffix (`std::str::FromStr`,
/// `core::str::FromStr`) — a user trait like `MyFromStr` is not matched.
fn is_in_fromstr_impl(node: Node, source: &[u8]) -> bool {
    // The Result must be the function's RETURN TYPE, not a body let binding.
    let Some(func) = enclosing_fn(node) else {
        return false;
    };
    let Some(ret) = func.child_by_field_name("return_type") else {
        return false;
    };
    if node.start_byte() < ret.start_byte() || node.end_byte() > ret.end_byte() {
        return false;
    }
    let Some(impl_item) = nearest_ancestor(node, "impl_item") else {
        return false;
    };
    let Some(trait_node) = impl_item.child_by_field_name("trait") else {
        return false;
    };
    let Ok(trait_text) = trait_node.utf8_text(source) else {
        return false;
    };
    trait_text == "FromStr" || trait_text.ends_with("::FromStr")
}

/// True when this `Result<_, ()>` is an idiomatic axum/tower use of the unit
/// error, where `()` is a valid `IntoResponse` rather than a discarded error.
///
/// `node` is the `Result<Ok, ()>` `generic_type` already confirmed to have a
/// `unit_type` error. We exempt three structural markers:
/// - the ok-type is `impl IntoResponse` (the `()` is a valid response body);
/// - the enclosing function carries `#[debug_handler]` (an axum handler);
/// - the enclosing `impl FromRequest`/`FromRequestParts` declares
///   `type Rejection = ()` (an extractor with a trivial rejection).
fn is_axum_unit_response(node: Node, source: &[u8]) -> bool {
    ok_type_is_into_response(node, source)
        || fn_has_debug_handler_attr(node, source)
        || in_from_request_impl_with_unit_rejection(node, source)
}

/// True when the ok-type (first positional arg) of `Result<Ok, ()>` is
/// `impl IntoResponse` — i.e. an `abstract_type` whose trait bound names
/// `IntoResponse`.
fn ok_type_is_into_response(result_node: Node, source: &[u8]) -> bool {
    let Some(args) = result_node.child_by_field_name("type_arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    let Some(ok_type) = args
        .named_children(&mut cursor)
        .find(|c| c.kind() != "type_binding")
    else {
        return false;
    };
    if ok_type.kind() != "abstract_type" {
        return false;
    }
    ok_type
        .utf8_text(source)
        .is_ok_and(|t| t.contains("IntoResponse"))
}

/// True when the function enclosing `node` carries a `#[debug_handler]` /
/// `#[axum::debug_handler]` attribute (an axum handler the macro validates).
fn fn_has_debug_handler_attr(node: Node, source: &[u8]) -> bool {
    let Some(func) = nearest_ancestor(node, "function_item") else {
        return false;
    };
    let mut sibling = func.prev_named_sibling();
    while let Some(s) = sibling {
        if s.kind() != "attribute_item" {
            break;
        }
        if s.utf8_text(source)
            .is_ok_and(|t| t.contains("debug_handler"))
        {
            return true;
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True when `node` sits in an `impl FromRequest`/`FromRequestParts` block that
/// declares `type Rejection = ();` — an axum extractor whose rejection is the
/// trivial unit response.
fn in_from_request_impl_with_unit_rejection(node: Node, source: &[u8]) -> bool {
    let Some(impl_item) = nearest_ancestor(node, "impl_item") else {
        return false;
    };
    let Some(trait_node) = impl_item.child_by_field_name("trait") else {
        return false;
    };
    let Ok(trait_text) = trait_node.utf8_text(source) else {
        return false;
    };
    if !(trait_text.contains("FromRequestParts") || trait_text.contains("FromRequest")) {
        return false;
    }
    let Some(body) = impl_item.child_by_field_name("body") else {
        return false;
    };
    let mut cursor = body.walk();
    body.named_children(&mut cursor).any(|item| {
        item.kind() == "type_item"
            && item
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                == Some("Rejection")
            && item
                .child_by_field_name("type")
                .is_some_and(|t| t.kind() == "unit_type")
    })
}

/// Nearest ancestor of `node` whose kind matches `kind`, or `None`.
fn nearest_ancestor<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == kind {
            return Some(ancestor);
        }
        current = ancestor.parent();
    }
    None
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

    fn run_on_src(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "src/svc.rs")
    }

    #[test]
    fn flags_result_unit_error_in_return() {
        assert_eq!(run_on("fn f() -> Result<i32, ()> { Ok(0) }").len(), 1);
    }

    #[test]
    fn flags_result_unit_error_in_field() {
        assert_eq!(run_on("struct S { last: Result<u8, ()> }").len(), 1);
    }

    #[test]
    fn allows_result_with_real_error() {
        assert!(run_on("fn f() -> Result<i32, String> { Ok(0) }").is_empty());
    }

    #[test]
    fn allows_io_result_alias() {
        // `io::Result<T>` only takes one type arg — we can't see the
        // error from the AST so we don't flag it.
        assert!(run_on("fn f() -> io::Result<()> { Ok(()) }").is_empty());
    }

    // --- axum/tower: `()` deliberately implements `IntoResponse` (#1262) ---

    #[test]
    fn allows_into_response_ok_type_with_unit_error() {
        // `Result<impl IntoResponse, ()>` axum handler — `()` is a valid
        // response, not a discarded error.
        assert!(
            run_on(
                "#[debug_handler]\n\
                 async fn h() -> Result<impl IntoResponse, ()> { Ok(()) }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_debug_handler_with_unit_error() {
        // `#[debug_handler]` marks an axum handler even when the ok-type is a
        // concrete response type rather than `impl IntoResponse`.
        assert!(
            run_on(
                "#[axum::debug_handler]\n\
                 async fn h() -> Result<MyResponse, ()> { Ok(MyResponse) }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_from_request_extractor_with_unit_rejection() {
        // Extractor whose `type Rejection = ()` — `Result<Self, ()>` is the
        // idiomatic "never rejects" shape.
        assert!(
            run_on(
                "impl<S> FromRequestParts<S> for A {\n\
                 \x20   type Rejection = ();\n\
                 \x20   async fn from_request_parts(_p: &mut Parts, _s: &S) \
                 -> Result<Self, ()> { unimplemented!() }\n\
                 }"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_plain_unit_error_without_axum_markers() {
        // Negative space: a plain parser with no axum/IntoResponse/Rejection
        // marker is still flagged.
        assert_eq!(run_on("fn parse() -> Result<Foo, ()> { Ok(Foo) }").len(), 1);
    }

    // --- test context: mock types idiomatically use `()` errors (#3891) ---

    #[test]
    fn allows_unit_error_in_cfg_test_mod_mock_service() {
        // #3891: a mock `tower::Service` impl inside a `#[cfg(test)] mod tests`
        // block in a `src/` file uses `type Error = ()` and a unit-error
        // future; the test never exercises the error path, so neither fires.
        assert!(
            run_on_src(
                "#[cfg(test)]\n\
                 mod tests {\n\
                 \x20   struct TestSvc;\n\
                 \x20   impl Service<Request<()>> for TestSvc {\n\
                 \x20       type Response = ();\n\
                 \x20       type Error = ();\n\
                 \x20       type Future = std::future::Ready<Result<(), ()>>;\n\
                 \x20   }\n\
                 }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unit_error_in_plain_test_fn() {
        // The `#[test]` form: a unit-error result declared inside a test
        // function is exempt as well.
        assert!(
            run_on_src(
                "#[test]\n\
                 fn t() {\n\
                 \x20   let _r: Result<(), ()> = Ok(());\n\
                 }"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_production_unit_error_in_src_file() {
        // Load-bearing guard: a production fn in a `src/` file with no
        // `#[test]`/`#[cfg(test)]` still fires.
        assert_eq!(run_on_src("fn f() -> Result<(), ()> { Ok(()) }").len(), 1);
    }

    #[test]
    fn flags_unit_error_in_from_request_impl_with_non_unit_rejection() {
        // The extractor exemption is scoped to a *unit* rejection; a method
        // returning `Result<_, ()>` next to `type Rejection = String` is still
        // a discarded error.
        assert_eq!(
            run_on(
                "impl<S> FromRequestParts<S> for A {\n\
                 \x20   type Rejection = String;\n\
                 \x20   fn helper() -> Result<u8, ()> { Ok(0) }\n\
                 }"
            )
            .len(),
            1
        );
    }

    // --- clippy-allow: author formally dismissed `result_unit_err` (#3735) ---

    #[test]
    fn allows_unit_error_under_expect_clippy_attr() {
        // #3735: `#[expect(clippy::result_unit_err)]` on the function suppresses
        // the equivalent clippy lint, so comply honors it.
        assert!(
            run_on_src(
                "#[expect(clippy::result_unit_err)]\n\
                 pub fn f() -> Result<(), ()> { Ok(()) }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unit_error_under_allow_clippy_attr() {
        // The `#[allow(...)]` form suppresses it just as `#[expect(...)]` does.
        assert!(
            run_on_src(
                "#[allow(clippy::result_unit_err)]\n\
                 fn g() -> Result<(), ()> { Ok(()) }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unit_error_for_issue_from_file_path_shape() {
        // The issue's exact shape: a `pub fn` returning a wrapped upstream
        // `Result<_, ()>` under `#[expect(clippy::result_unit_err)]`.
        assert!(
            run_on_src(
                "#[expect(clippy::result_unit_err)]\n\
                 pub fn from_file_path(path: &str) -> Result<Self, ()> {\n\
                 \x20   Ok(Self(url::Url::from_file_path(path)?))\n\
                 }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unit_error_under_crate_level_inner_attr() {
        // A crate-root `#![allow(clippy::result_unit_err)]` suppresses every
        // `Result<_, ()>` in the file.
        assert!(
            run_on_src(
                "#![allow(clippy::result_unit_err)]\n\
                 fn h() -> Result<(), ()> { Ok(()) }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unit_error_under_enclosing_impl_allow() {
        // The allow on an enclosing `impl` block reaches a method's
        // `Result<_, ()>`.
        assert!(
            run_on_src(
                "#[allow(clippy::result_unit_err)]\n\
                 impl Foo { fn m() -> Result<(), ()> { Ok(()) } }"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_unit_error_without_any_allow() {
        // Load-bearing guard: no suppression at all still fires.
        assert_eq!(
            run_on_src("fn p() -> Result<(), ()> { Ok(()) }").len(),
            1
        );
    }

    #[test]
    fn flags_unit_error_with_unrelated_allow() {
        // Load-bearing guard: an allow that does not name `result_unit_err`
        // (here `dead_code`) does not suppress the finding.
        assert_eq!(
            run_on_src(
                "#[allow(dead_code)]\n\
                 fn q() -> Result<(), ()> { Ok(()) }"
            )
            .len(),
            1
        );
    }

    // --- trait contract: `Result<(), ()>` is a binary ok/err signal (#4442) ---

    #[test]
    fn allows_unit_unit_result_in_trait_definition() {
        // gRPC/transport trait interface: `Result<(), ()>` is a binary
        // success/failure event, error detail travels out-of-band.
        assert!(
            run_on_src("pub trait T { fn f(&self) -> Result<(), ()>; }").is_empty()
        );
    }

    #[test]
    fn allows_unit_unit_result_in_trait_impl() {
        // The impl conforms to the trait contract, so it is exempt too.
        assert!(
            run_on_src(
                "impl T for S { fn f(&self) -> Result<(), ()> { Ok(()) } }"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_unit_unit_result_in_free_fn() {
        // Load-bearing negative: a free function is not a trait contract, so
        // `Result<(), ()>` stays flagged (the exemption is not blanket).
        assert_eq!(run_on_src("fn f() -> Result<(), ()> { Ok(()) }").len(), 1);
    }

    #[test]
    fn flags_unit_unit_result_in_inherent_impl() {
        // Load-bearing negative: an inherent-impl method is not a trait
        // contract, so it stays flagged.
        assert_eq!(
            run_on_src("impl S { fn f(&self) -> Result<(), ()> { Ok(()) } }").len(),
            1
        );
    }

    #[test]
    fn flags_value_unit_result_in_trait_definition() {
        // Load-bearing negative: `Result<i32, ()>` returns real data while
        // discarding the error detail — a stronger smell that stays flagged
        // even inside a trait contract.
        assert_eq!(
            run_on_src("pub trait T { fn f(&self) -> Result<i32, ()>; }").len(),
            1
        );
    }

    // --- borrowed narrowing accessor: `Result<&T, ()>` is `Option`-like (#4463) ---

    #[test]
    fn allows_to_str_borrowed_narrowing_accessor() {
        // The warp repro: `to_str` returns a borrowed `&str` view, `Err(())`
        // means "not a text message" — a closed single-element failure set.
        assert!(
            run_on_src(
                "impl M {\n\
                 \x20   pub fn to_str(&self) -> Result<&str, ()> {\n\
                 \x20       match self.inner {\n\
                 \x20           X::Text(ref s) => Ok(s),\n\
                 \x20           _ => Err(()),\n\
                 \x20       }\n\
                 \x20   }\n\
                 }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_as_bytes_borrowed_narrowing_accessor() {
        // `as_` prefix with a `&[u8]` borrowed view.
        assert!(
            run_on_src("fn as_bytes(&self) -> Result<&[u8], ()> { Err(()) }").is_empty()
        );
    }

    #[test]
    fn allows_try_as_text_borrowed_narrowing_accessor() {
        // `try_` prefix with a `&str` borrowed view.
        assert!(
            run_on_src("fn try_as_text(&self) -> Result<&str, ()> { Err(()) }").is_empty()
        );
    }

    #[test]
    fn flags_borrowed_result_without_accessor_prefix() {
        // Load-bearing negative: a reference ok-type but the name is not a
        // `to_`/`as_`/`try_` accessor, so it stays flagged.
        assert_eq!(
            run_on_src("fn parse(&self) -> Result<&str, ()> { Err(()) }").len(),
            1
        );
    }

    #[test]
    fn flags_owned_return_with_accessor_prefix() {
        // Load-bearing negative: a `to_` accessor whose ok-type is owned
        // (`Config`, not a reference) — the reference requirement is what makes
        // it a borrowed narrowing view, so it stays flagged.
        assert_eq!(
            run_on_src("fn to_config(&self) -> Result<Config, ()> { Err(()) }").len(),
            1
        );
    }

    #[test]
    fn flags_borrowed_result_in_struct_field() {
        // Load-bearing negative: a `Result<&'static str, ()>` struct field is
        // not inside a function return type, so it stays flagged.
        assert_eq!(
            run_on_src("struct S { x: Result<&'static str, ()> }").len(),
            1
        );
    }

    // --- logos lexer callback: `Result<T, ()>` is library-mandated (#5119) ---

    #[test]
    fn allows_logos_callback_with_mut_lexer_param() {
        // The logos repro: a `#[regex(..., lex_single_line_string)]` callback
        // takes `&mut Lexer<Token>` and returns `Result<String, ()>`; `Err(())`
        // is logos's "reject this token match", so the `()` is mandated.
        assert!(
            run_on_src(
                "pub fn lex_single_line_string(lexer: &mut Lexer<Token>) \
                 -> Result<String, ()> { Err(()) }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_logos_callback_with_fully_qualified_lexer() {
        // logos generates callbacks against `logos::Lexer<'s, Self>`; the
        // scoped path is recognized too.
        assert!(
            run_on_src(
                "fn cb<'s>(lex: &mut logos::Lexer<'s, Token>) \
                 -> Result<u8, ()> { Err(()) }"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_unit_error_without_lexer_param() {
        // Load-bearing negative: a `&mut Foo<Token>` param is not a logos
        // lexer, so the unit error stays flagged.
        assert_eq!(
            run_on_src(
                "fn f(x: &mut Foo<Token>) -> Result<String, ()> { Err(()) }"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_unit_error_with_shared_ref_lexer_param() {
        // Load-bearing negative: logos callbacks take `&mut Lexer`; a shared
        // `&Lexer<Token>` is not the callback signature, so it stays flagged.
        assert_eq!(
            run_on_src("fn f(x: &Lexer<Token>) -> Result<String, ()> { Err(()) }").len(),
            1
        );
    }

    #[test]
    fn flags_unit_error_struct_field_next_to_lexer_fn() {
        // Load-bearing negative: the exemption is scoped to the function's
        // return type; a struct field stays flagged even in a logos crate.
        assert_eq!(
            run_on_src("struct S { last: Result<u8, ()> }").len(),
            1
        );
    }

    #[test]
    fn flags_unit_error_with_bare_non_generic_lexer_param() {
        // Load-bearing negative: the logos callback parameter is `Lexer<…>`; a
        // bare non-generic `&mut Lexer` is not the callback signature, so it
        // stays flagged.
        assert_eq!(
            run_on_src("fn f(x: &mut Lexer) -> Result<String, ()> { Err(()) }").len(),
            1
        );
    }

    #[test]
    fn allows_logos_callback_method_with_self_and_lexer() {
        // logos callbacks can be methods: a `(&self, lex: &mut Lexer<T>)`
        // receiver alongside the lexer parameter is still exempt — the `self`
        // parameter is skipped, the `&mut Lexer<T>` parameter matches.
        assert!(
            run_on_src(
                "impl C {\n\
                 \x20   fn cb(&self, lex: &mut Lexer<Token>) \
                 -> Result<u8, ()> { Err(()) }\n\
                 }"
            )
            .is_empty()
        );
    }

    // --- FromStr trait: `type Err = ()` is idiomatic, Option inapplicable (#6389) ---

    #[test]
    fn allows_unit_error_in_fromstr_impl() {
        // The insta repro: `impl std::str::FromStr` mandates `Result<Self,
        // Self::Err>`; `type Err = ()` is the idiomatic no-detail error type and
        // `Option` cannot satisfy the trait contract.
        assert!(
            run_on_src(
                "impl std::str::FromStr for TestRunner {\n\
                 \x20   type Err = ();\n\
                 \x20   fn from_str(value: &str) -> Result<TestRunner, ()> {\n\
                 \x20       match value {\n\
                 \x20           \"auto\" => Ok(TestRunner::Auto),\n\
                 \x20           _ => Err(()),\n\
                 \x20       }\n\
                 \x20   }\n\
                 }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unit_error_in_bare_fromstr_impl() {
        // A bare `impl FromStr` (no path qualifier) is the same trait contract.
        assert!(
            run_on_src(
                "impl FromStr for Color {\n\
                 \x20   type Err = ();\n\
                 \x20   fn from_str(s: &str) -> Result<Color, ()> { Err(()) }\n\
                 }"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_unit_error_in_free_parse_fn() {
        // Load-bearing negative: a free function returning `Result<u32, ()>` is
        // not a `FromStr` impl, so it stays flagged.
        assert_eq!(
            run_on_src("fn parse(s: &str) -> Result<u32, ()> { Err(()) }").len(),
            1
        );
    }

    #[test]
    fn flags_unit_error_in_non_fromstr_trait_impl() {
        // Load-bearing negative: a method inside a non-`FromStr` trait impl
        // returning `Result<u32, ()>` stays flagged — the exemption is scoped to
        // the `FromStr` contract.
        assert_eq!(
            run_on_src(
                "impl SomeOtherTrait for T {\n\
                 \x20   fn from_str(s: &str) -> Result<u32, ()> { Err(()) }\n\
                 }"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_unit_error_in_fromstr_body_let_binding() {
        // Load-bearing negative: the exemption is scoped to from_str's return
        // type; a `Result<u32, ()>` let binding inside the body is a genuine
        // discarded error and stays flagged.
        assert_eq!(
            run_on_src(
                "impl FromStr for T {\n\
                 \x20   type Err = ();\n\
                 \x20   fn from_str(s: &str) -> Result<T, ()> {\n\
                 \x20       let n: Result<u32, ()> = inner(s);\n\
                 \x20       n.map(T)\n\
                 \x20   }\n\
                 }"
            )
            .len(),
            1
        );
    }

    #[test]
    fn flags_unit_error_in_user_fromstr_named_trait_impl() {
        // Load-bearing negative: a user trait `MyFromStr` ends in `FromStr` but
        // is not the std trait; the path-segment boundary excludes it, so it
        // stays flagged.
        assert_eq!(
            run_on_src(
                "impl MyFromStr for T {\n\
                 \x20   fn from_str(s: &str) -> Result<u32, ()> { Err(()) }\n\
                 }"
            )
            .len(),
            1
        );
    }

    // --- local `Result` alias reorders T/E positions (#5544) ---

    #[test]
    fn allows_unit_in_file_with_local_result_alias_swapping_params() {
        // The wgpu/naga repro: a file-local `type Result<'a, T> =
        // core::result::Result<T, Box<Error<'a>>>` puts the success type first,
        // so `Result<'static, ()>` has `()` as the success type, not the error.
        assert!(
            run_on_src(
                "type Result<'a, T> = core::result::Result<T, Box<Error<'a>>>;\n\
                 fn set() -> Result<'static, ()> { Ok(()) }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unit_in_file_with_std_result_alias() {
        // The minimal alias form `type Result<'a, T> =
        // std::result::Result<T, Error>` — `Result<'a, ()>` is `Ok(())`.
        assert!(
            run_on_src(
                "type Result<'a, T> = std::result::Result<T, Error>;\n\
                 fn f<'a>() -> Result<'a, ()> { Ok(()) }"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_unit_when_result_alias_lives_in_nested_mod() {
        // The alias is detected anywhere in the file, including a nested `mod`.
        assert!(
            run_on_src(
                "mod inner {\n\
                 \x20   type Result<T> = std::result::Result<T, Error>;\n\
                 \x20   fn f() -> Result<()> { Ok(()) }\n\
                 }"
            )
            .is_empty()
        );
    }

    #[test]
    fn flags_genuine_std_unit_error_without_local_result_alias() {
        // Load-bearing negative: a genuine `std::result::Result<T, ()>` in a
        // file with no local `Result` alias still flags — the `()` really is
        // the error type there.
        assert_eq!(
            run_on_src("fn g() -> std::result::Result<i32, ()> { Ok(0) }").len(),
            1
        );
    }

    // --- private type alias: nom/winnow parser-combinator idiom (#6965) ---

    #[test]
    fn allows_unit_error_in_private_type_alias_rhs() {
        // The gitoxide repro: a module-private `type ParseResult<T> =
        // Result<T, ()>` parser-combinator alias where `Err(())` is the "input
        // did not match" rejection signal. No visibility modifier → exempt.
        assert!(run_on_src("type ParseResult<T> = Result<T, ()>;").is_empty());
    }

    #[test]
    fn allows_unit_error_in_private_iresult_alias() {
        // The nom-style `type IResult<T> = Result<T, ()>` alias is the same
        // idiom and is exempt for the same reason.
        assert!(run_on_src("type IResult<T> = Result<T, ()>;").is_empty());
    }

    #[test]
    fn flags_unit_error_in_pub_type_alias() {
        // Load-bearing negative: a `pub` alias is part of the public surface —
        // it can mislead a consumer, so it stays flagged.
        assert_eq!(
            run_on_src("pub type ParseResult<T> = Result<T, ()>;").len(),
            1
        );
    }

    #[test]
    fn flags_unit_error_in_pub_crate_type_alias() {
        // Load-bearing negative: any visibility modifier (here `pub(crate)`)
        // makes the alias non-exempt — only a fully module-private alias is.
        assert_eq!(
            run_on_src("pub(crate) type ParseResult<T> = Result<T, ()>;").len(),
            1
        );
    }

    #[test]
    fn flags_unit_error_in_function_return_not_alias() {
        // Load-bearing negative: a direct `Result<_, ()>` in a function return
        // type is not a `type_item` RHS, so it stays flagged.
        assert_eq!(
            run_on_src("fn parse() -> Result<u8, ()> { Err(()) }").len(),
            1
        );
    }

    #[test]
    fn flags_unit_error_nested_under_private_alias_rhs() {
        // Load-bearing negative: the exemption is the alias's RHS *itself*; a
        // `Result<_, ()>` nested as a generic argument under the RHS (here under
        // `Box`) is not the declared alias type, so it stays flagged.
        assert_eq!(
            run_on_src("type Foo<T> = Box<Result<T, ()>>;").len(),
            1
        );
    }

    #[test]
    fn flags_unit_error_in_associated_type_in_trait_impl() {
        // Load-bearing negative: an associated `type Assoc = Result<_, ()>;` in
        // an `impl`/`trait` body is also a `type_item` with no visibility
        // modifier, but it is a trait-contract member, not a private module
        // parser alias, so it stays flagged.
        assert_eq!(
            run_on_src("impl Tr for T { type Assoc = Result<Value, ()>; }").len(),
            1
        );
    }
}
