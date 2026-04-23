//! exception-use-error-cause backend — inside a `catch_clause`, flag any
//! `throw new Error(...)` (or Error subclass) whose arguments don't include
//! a `cause:` field. This is stricter than `error-without-cause`: the
//! presence of a surrounding catch block is the signal that a caught
//! error is available to attach.
//!
//! Skipped when `new Error` has zero arguments (the body didn't capture
//! anything — likely a placeholder).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const ERROR_CTORS: &[&str] = &[
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "ReferenceError",
    "EvalError",
    "URIError",
    "AggregateError",
];

const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

/// Returns true if `node` lives inside a `catch_clause`, stopping at
/// function boundaries (a catch in the outer function doesn't help an
/// inner function's throw).
fn is_inside_catch(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "catch_clause" {
            return true;
        }
        if FUNCTION_KINDS.contains(&n.kind()) {
            return false;
        }
        current = n.parent();
    }
    false
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            if node.kind() != "throw_statement" {
                return;
            }
            let Some(arg) = node.named_child(0) else { return };
            if arg.kind() != "new_expression" {
                return;
            }
            let Some(ctor) = arg.child_by_field_name("constructor") else { return };
            let Ok(ctor_name) = ctor.utf8_text(source) else { return };
            if !ERROR_CTORS.contains(&ctor_name) {
                return;
            }
            if !is_inside_catch(node) {
                return;
            }
            // Must have at least one argument — a bare `new Error()` is a
            // placeholder, skip.
            let Some(args) = arg.child_by_field_name("arguments") else { return };
            if args.named_child_count() == 0 {
                return;
            }
            let args_text = args.utf8_text(source).unwrap_or("");
            if args_text.contains("cause:") || args_text.contains("cause :") {
                return;
            }

            let pos = arg.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "exception-use-error-cause".into(),
                message: format!(
                    "`throw new {ctor_name}(...)` inside catch drops the original \
                     stack trace — pass `{{ cause: e }}` as the second argument."
                ),
                severity: Severity::Warning,
                span: None,
            });
        });
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_rethrow_without_cause() {
        let d = run_on("try { x(); } catch (e) { throw new Error('boom'); }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "exception-use-error-cause");
    }

    #[test]
    fn flags_rethrow_typeerror_without_cause() {
        let d = run_on("try { x(); } catch (e) { throw new TypeError('boom'); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_rethrow_with_cause() {
        assert!(
            run_on("try { x(); } catch (e) { throw new Error('boom', { cause: e }); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_throw_outside_catch() {
        assert!(run_on("function f() { throw new Error('boom'); }").is_empty());
    }

    #[test]
    fn allows_throw_existing_error() {
        // Re-throwing the caught error directly — not a wrap.
        assert!(run_on("try { x(); } catch (e) { throw e; }").is_empty());
    }

    #[test]
    fn allows_bare_new_error() {
        // No args — placeholder, not a wrap.
        assert!(run_on("try { x(); } catch (e) { throw new Error(); }").is_empty());
    }
}
