//! error-without-cause backend for TypeScript / JavaScript / TSX.
//!
//! Detects `new Error(<expr>.message)` (or any built-in Error subclass) where
//! the second argument is missing or doesn't contain a `cause` field. The
//! pattern signals "I'm wrapping a caught error" — `.message` access tells
//! us we have the source error in scope, so omitting `cause` is a clear
//! mistake. We deliberately don't flag `new Error("literal")` — that's a
//! fresh error, not a wrap.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const ERROR_CTORS: &[&str] = &[
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "EvalError",
    "URIError",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Error", "TypeError", "RangeError", "SyntaxError", "ReferenceError", "EvalError", "URIError"])
    }

    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["new_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        // Constructor must be one of the built-in Error types.
        let Some(ctor) = node.child_by_field_name("constructor") else {
            return;
        };
        let Ok(ctor_name) = ctor.utf8_text(source) else {
            return;
        };
        if !ERROR_CTORS.contains(&ctor_name) {
            return;
        }
        // Arguments must contain a `.message` member access (the wrap signal)
        // and must NOT contain a `cause` field anywhere in args.
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let args_text = args.utf8_text(source).unwrap_or("");
        let wraps_existing = args_text.contains(".message");
        if !wraps_existing {
            return;
        }
        if args_text.contains("cause:") || args_text.contains("cause :") {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "error-without-cause".into(),
            message: format!(
                "`new {ctor_name}(...)` wraps an existing error but drops `cause`. \
                 Add `{{ cause: original }}` as the 2nd argument to preserve the \
                 stack trace and cause chain — debuggers and `error.cause` rely on it."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_wrap_without_cause() {
        let diags = run_on("try { f(); } catch (e) { throw new Error(e.message); }");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "error-without-cause");
    }

    #[test]
    fn allows_wrap_with_cause() {
        let diags = run_on(
            "try { f(); } catch (e) { throw new Error(e.message, { cause: e }); }",
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_fresh_error_with_literal() {
        let diags = run_on("throw new Error('boom');");
        assert!(diags.is_empty());
    }

    #[test]
    fn flags_typeerror_wrap() {
        let diags = run_on("catch (err) { throw new TypeError(err.message); }");
        assert_eq!(diags.len(), 1);
    }
}
