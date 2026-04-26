//! rust-block-on-in-async backend.
//!
//! Walks `call_expression` nodes whose function path ends in
//! `block_on` and verifies the call is inside an `async fn`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_inside_async_fn;

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(text) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !text.ends_with("block_on") {
        return;
    }
    if !is_inside_async_fn(node, source) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "rust-block-on-in-async".into(),
        message: format!(
            "`{text}(..)` from inside an `async fn` triggers tokio's `Cannot \
             start a runtime from within a runtime` panic. Use `.await` on the \
             future instead."
        ),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_rust(source, &Check)


    }

    #[test]
    fn flags_block_on_inside_async() {
        let source = "async fn f() { rt.block_on(other_future()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_futures_block_on_inside_async() {
        let source = "async fn f() { futures::executor::block_on(other()); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_block_on_in_main() {
        let source = "fn main() { rt.block_on(server()); }";
        assert!(run_on(source).is_empty());
    }
}
