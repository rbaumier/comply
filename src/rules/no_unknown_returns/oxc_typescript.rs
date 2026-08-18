//! no-unknown-returns oxc backend — flag a return annotation that resolves to
//! `unknown`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::unknown_type::produces_unknown;
use oxc_ast::ast::TSTypeAnnotation;
use std::sync::Arc;

/// The declared return type of any node that carries one. A node with no
/// annotation returns `None`: an inferred return is not a declared contract.
fn return_annotation<'a>(node: &oxc_semantic::AstNode<'a>) -> Option<&'a TSTypeAnnotation<'a>> {
    match node.kind() {
        AstKind::Function(function) => function.return_type.as_deref(),
        AstKind::ArrowFunctionExpression(arrow) => arrow.return_type.as_deref(),
        AstKind::TSMethodSignature(method) => method.return_type.as_deref(),
        AstKind::TSCallSignatureDeclaration(signature) => signature.return_type.as_deref(),
        AstKind::TSConstructSignatureDeclaration(signature) => signature.return_type.as_deref(),
        AstKind::TSFunctionType(function) => Some(&function.return_type),
        AstKind::TSConstructorType(constructor) => Some(&constructor.return_type),
        _ => None,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Function,
            AstType::ArrowFunctionExpression,
            AstType::TSMethodSignature,
            AstType::TSCallSignatureDeclaration,
            AstType::TSConstructSignatureDeclaration,
            AstType::TSFunctionType,
            AstType::TSConstructorType,
        ]
    }

    /// The keyword has to be written somewhere in the file for any annotation
    /// to resolve to it — alias resolution never leaves the file.
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["unknown"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(annotation) = return_annotation(node) else {
            return;
        };
        if !produces_unknown(&annotation.type_annotation, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, annotation.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "a declared `unknown` return hands the caller an unparsed value — \
                      return a named domain type, and parse the value where it is produced."
                .into(),
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_function_declaration() {
        assert_eq!(run("function f(): unknown { return 1; }").len(), 1);
    }

    #[test]
    fn flags_promise_of_unknown() {
        assert_eq!(
            run("async function f(): Promise<unknown> { return 1; }").len(),
            1
        );
    }

    #[test]
    fn flags_arrow() {
        assert_eq!(run("const f = (): unknown => 1;").len(), 1);
    }

    #[test]
    fn flags_method_signature() {
        assert_eq!(run("interface I { m(): unknown }").len(), 1);
    }

    #[test]
    fn flags_alias() {
        assert_eq!(
            run("type A = unknown; function f(): A { return 1; }").len(),
            1
        );
    }

    #[test]
    fn flags_transitive_alias() {
        assert_eq!(
            run("type A = B; type B = unknown; function f(): A { return 1; }").len(),
            1
        );
    }

    #[test]
    fn flags_union_member() {
        assert_eq!(run("function f(): string | unknown { return ''; }").len(), 1);
    }

    #[test]
    fn flags_function_type() {
        assert_eq!(run("type F = () => unknown;").len(), 1);
    }

    #[test]
    fn flags_exported_alias() {
        assert_eq!(
            run("export type A = unknown; export function f(): A { return 1; }").len(),
            1
        );
    }

    #[test]
    fn ignores_inferred_return() {
        assert!(run("function f() { return unknownValue; }").is_empty());
    }

    #[test]
    fn ignores_void() {
        assert!(run("function f(): void { work(); }").is_empty());
    }

    #[test]
    fn ignores_array_of_unknown() {
        assert!(run("function f(): unknown[] { return []; }").is_empty());
    }

    #[test]
    fn ignores_dictionary() {
        assert!(run("function f(): Record<string, unknown> { return {}; }").is_empty());
    }

    #[test]
    fn ignores_generic_alias() {
        assert!(run("type Box<T> = T; function f(): Box<unknown> { return 1; }").is_empty());
    }

    #[test]
    fn ignores_alias_cycle() {
        assert!(run("type A = B; type B = A; function f(): A { return 1 as A; }").is_empty());
    }

    #[test]
    fn ignores_type_parameter_shadowing_an_alias() {
        assert!(run("type T = unknown; function f<T>(value: T): T { return value; }").is_empty());
    }

    #[test]
    fn ignores_parameter_annotation() {
        assert!(run("function f(x: unknown) {}").is_empty());
    }
}
