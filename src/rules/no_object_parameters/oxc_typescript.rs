//! no-object-parameters oxc backend — flag `TSObjectKeyword` in a parameter's
//! own type annotation.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, enclosing_parameter};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSObjectKeyword]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSObjectKeyword(kw) = node.kind() else {
            return;
        };
        if enclosing_parameter(node, semantic).is_none() {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, kw.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`object` accepts every non-primitive value and exposes no property — \
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
        assert_eq!(run("function save(value: object) {}").len(), 1);
    }

    #[test]
    fn flags_arrow_parameter() {
        assert_eq!(run("const f = (x: object) => x;").len(), 1);
    }

    #[test]
    fn flags_ambient_declaration() {
        assert_eq!(run("declare function g(x: object): void;").len(), 1);
    }

    #[test]
    fn flags_method_signature() {
        assert_eq!(run("interface Store { put(x: object): void }").len(), 1);
    }

    #[test]
    fn flags_constructor_parameter_property() {
        assert_eq!(
            run("class S { constructor(private readonly cfg: object) {} }").len(),
            1
        );
    }

    #[test]
    fn flags_default_valued_parameter() {
        assert_eq!(run("function h(x: object = {}) {}").len(), 1);
    }

    #[test]
    fn flags_union_member() {
        assert_eq!(run("function k(x: string | object) {}").len(), 1);
    }

    #[test]
    fn flags_parenthesized_type() {
        assert_eq!(run("function k(x: (object)) {}").len(), 1);
    }

    #[test]
    fn flags_function_type_parameter() {
        assert_eq!(run("type Handler = (x: object) => void;").len(), 1);
    }

    #[test]
    fn flags_destructured_parameter() {
        assert_eq!(run("function f({ a }: object) { return a; }").len(), 1);
    }

    #[test]
    fn ignores_dictionary_value() {
        // `Record<string, object>` is the dictionary-value position, owned by
        // `no-unsafe-dictionary-type`.
        assert!(run("function f(x: Record<string, object>) {}").is_empty());
    }

    #[test]
    fn ignores_unknown_parameter() {
        assert!(run("function f(x: unknown) {}").is_empty());
    }

    #[test]
    fn ignores_empty_object_type() {
        assert!(run("function f(x: {}) {}").is_empty());
    }

    #[test]
    fn ignores_intersection_constraint() {
        assert!(run("function f<T>(x: T & object) {}").is_empty());
    }

    #[test]
    fn ignores_return_type() {
        assert!(run("function f(): object { return {}; }").is_empty());
    }

    #[test]
    fn ignores_variable_annotation() {
        assert!(run("const x: object = {};").is_empty());
    }

    #[test]
    fn ignores_alias_declaration() {
        // The alias itself is `redundant-type-aliases`' span.
        assert!(run("type Bag = object; function f(x: Bag) {}").is_empty());
    }

    #[test]
    fn ignores_type_argument_and_array_positions() {
        assert!(run("function f(x: Array<object>) {}").is_empty());
        assert!(run("function i(...rest: object[]) {}").is_empty());
    }

    #[test]
    fn ignores_type_literal_property() {
        assert!(run("function f(x: { meta: object }) { return x; }").is_empty());
    }
}
