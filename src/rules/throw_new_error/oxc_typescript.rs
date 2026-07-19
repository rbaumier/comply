//! throw-new-error OXC backend — flag error-constructor calls without `new`.
//!
//! Fires only when the callee genuinely resolves to a `class` declaration (a
//! real constructor) or is an unbound built-in global error constructor. A
//! callee whose name ends in `Error` but resolves to a function/factory binding
//! is a factory that returns an Error instance (`createError`, `H3LibraryError`)
//! — calling it without `new` is correct, so it is never flagged. The name alone
//! is never treated as evidence: an unresolvable callee is left alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// Built-in global error constructors. When a callee with one of these names has
/// no local binding it is the global constructor and `new` is required.
const BUILTIN_ERROR_CONSTRUCTORS: &[&str] = &[
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "EvalError",
    "ReferenceError",
    "URIError",
    "AggregateError",
];

/// Matches PascalCase names ending in "Error": Error, TypeError, MyCustomError, etc.
fn is_error_like(name: &str) -> bool {
    if !name.ends_with("Error") || name.is_empty() {
        return false;
    }
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// Whether the callee identifier genuinely creates an error instance when
/// prefixed with `new` — i.e. it resolves to a `class` declaration, or it is an
/// unbound built-in global error constructor. A callee that resolves to a
/// function/arrow binding is a factory (`createError`, `H3LibraryError`) and is
/// rejected; an unresolvable callee is rejected too (the name is not evidence).
fn callee_is_constructor(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let scoping = semantic.scoping();
    let resolved_symbol = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id());

    let Some(sym_id) = resolved_symbol else {
        // No local binding: a built-in global error constructor requires `new`,
        // anything else is left alone (the `*Error` name is not evidence).
        return BUILTIN_ERROR_CONSTRUCTORS.contains(&ident.name.as_str());
    };

    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
        .any(|kind| matches!(kind, AstKind::Class(_)))
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        match &call.callee {
            // Direct call: `Error('x')` / `FooError('x')`.
            Expression::Identifier(id) => {
                if !is_error_like(id.name.as_str()) {
                    return;
                }
                if !callee_is_constructor(id, semantic) {
                    return;
                }
            }
            // Member access: `module.CustomError('x')`. The member callee carries
            // no resolvable local binding, so it is never flagged.
            Expression::StaticMemberExpression(_) => return,
            _ => return,
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `new` when creating an error.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- Built-in global error constructors without `new` still flag ---

    #[test]
    fn flags_builtin_error_without_new() {
        assert_eq!(run_on("throw Error('boom');").len(), 1);
    }

    #[test]
    fn flags_builtin_typeerror_without_new() {
        assert_eq!(run_on("throw TypeError('bad');").len(), 1);
    }

    // --- In-scope error class without `new` still flags ---

    #[test]
    fn flags_in_scope_error_class_without_new() {
        let src = "class FooError extends Error {}\nthrow FooError('x');";
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // --- `new` is always correct: never flagged ---

    #[test]
    fn allows_builtin_error_with_new() {
        assert!(run_on("throw new Error('boom');").is_empty());
    }

    #[test]
    fn allows_in_scope_error_class_with_new() {
        let src = "class FooError extends Error {}\nthrow new FooError('x');";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // --- Regression #5669: factory functions returning Error instances ---

    // A function named `H3LibraryError` returns an Error instance; calling it
    // without `new` is correct by design (uber/h3-js lib/errors.js).
    #[test]
    fn allows_error_factory_function_call() {
        let src = "function H3LibraryError(code) { return createError(code); }\n\
                   const err = H3LibraryError(1);";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_error_factory_function_thrown() {
        let src = "function H3LibraryError(code) { return createError(code); }\n\
                   throw H3LibraryError(1);";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_arrow_error_factory_call() {
        let src = "const makeError = () => new Error('x');\n\
                   const FooError = makeError;\n\
                   throw FooError();";
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // --- Unresolvable / imported callee: name is not evidence, not flagged ---

    #[test]
    fn allows_unresolved_error_callee() {
        // No binding in scope for `SomeImportedError` — could be a factory.
        assert!(run_on("throw SomeImportedError('x');").is_empty());
    }

    // --- Member-expression callee: never flagged ---

    #[test]
    fn allows_member_error_callee() {
        assert!(run_on("throw module.CustomError('x');").is_empty());
    }

    // --- Non-error names never flagged ---

    #[test]
    fn allows_non_error_function_call() {
        assert!(run_on("foo();").is_empty());
    }
}
