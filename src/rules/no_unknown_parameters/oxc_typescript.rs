//! no-unknown-parameters oxc backend — flag `TSUnknownKeyword` in a parameter's
//! own type annotation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, enclosing_parameter};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, FormalParameter};
use std::sync::Arc;

/// The one parameter name that carries an `unknown` by contract: the second
/// argument of `new Error(message, { cause })` accepts any thrown value.
const CAUSE: &str = "cause";

fn is_cause(param: &FormalParameter) -> bool {
    match &param.pattern {
        BindingPattern::BindingIdentifier(id) => id.name.as_str() == CAUSE,
        _ => false,
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSUnknownKeyword]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSUnknownKeyword(kw) = node.kind() else {
            return;
        };
        let Some(param) = enclosing_parameter(node, semantic) else {
            return;
        };
        if is_cause(param) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, kw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "an `unknown` parameter defers the parse onto every caller — \
                      accept a named domain type and parse external input at its boundary."
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
    fn flags_function_declaration_parameter() {
        assert_eq!(run("function save(value: unknown) {}").len(), 1);
    }

    #[test]
    fn flags_arrow_parameter() {
        assert_eq!(run("const f = (x: unknown) => {};").len(), 1);
    }

    #[test]
    fn flags_method_signature() {
        assert_eq!(run("interface Store { put(x: unknown): void }").len(), 1);
    }

    #[test]
    fn flags_constructor_parameter_property() {
        assert_eq!(
            run("class S { constructor(private readonly cfg: unknown) {} }").len(),
            1
        );
    }

    #[test]
    fn flags_default_valued_parameter() {
        assert_eq!(run("function h(x: unknown = {}) {}").len(), 1);
    }

    #[test]
    fn flags_function_type_parameter() {
        assert_eq!(run("type F = (x: unknown) => void;").len(), 1);
    }

    #[test]
    fn flags_ambient_declaration() {
        assert_eq!(run("declare function g(x: unknown): void;").len(), 1);
    }

    /// A rest parameter must be an array type, so `unknown` never reaches one
    /// bare — only as the `unknown[]` container the rule leaves alone.
    #[test]
    fn ignores_rest_parameter() {
        assert!(run("function r(...rest: unknown[]) {}").is_empty());
    }

    #[test]
    fn flags_union_member() {
        assert_eq!(run("function k(x: string | unknown) {}").len(), 1);
    }

    #[test]
    fn ignores_parameter_named_cause() {
        assert!(run("function f(cause: unknown) {}").is_empty());
    }

    #[test]
    fn ignores_array_of_unknown() {
        assert!(run("function f(x: unknown[]) {}").is_empty());
    }

    #[test]
    fn ignores_generic_container() {
        assert!(run("function f(x: Array<unknown>) {}").is_empty());
    }

    #[test]
    fn ignores_dictionary() {
        assert!(run("function f(x: Record<string, unknown>) {}").is_empty());
    }

    #[test]
    fn ignores_named_type() {
        assert!(run("function f(x: string) {}").is_empty());
    }

    #[test]
    fn ignores_unannotated_parameter() {
        assert!(run("function f(x) {}").is_empty());
    }

    #[test]
    fn ignores_return_type() {
        assert!(run("function f(): unknown { return 1; }").is_empty());
    }

    #[test]
    fn ignores_variable_annotation() {
        assert!(run("const x: unknown = readInput();").is_empty());
    }

    #[test]
    fn ignores_catch_binding() {
        assert!(run("try { work(); } catch (e: unknown) {}").is_empty());
    }
}
