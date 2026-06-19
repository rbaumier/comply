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
//! Axum/tower exception: in that ecosystem `()` deliberately implements
//! `IntoResponse` (an empty `200 OK`), so `Result<_, ()>` is idiomatic for
//! handlers and extractors. We exempt the structurally-detectable cases:
//! an `impl IntoResponse` ok-type, a `#[debug_handler]` handler, and a
//! `FromRequest`/`FromRequestParts` extractor whose `type Rejection = ()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{is_in_test_context, result_error_type};
use tree_sitter::Node;

crate::ast_check! { on ["generic_type"] => |node, source, ctx, diagnostics|
    let Some(err_type) = result_error_type(node, source) else {
        return;
    };
    if err_type.kind() != "unit_type" {
        return;
    }
    if is_in_test_context(node, source) {
        return;
    }
    if is_axum_unit_response(node, source) {
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
}
