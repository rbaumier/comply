use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::TSType;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSIntersectionType]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSIntersectionType(intersection) = node.kind() else {
            return;
        };
        let has_useless = intersection.types.iter().any(|ty| {
            matches!(ty, TSType::TSUnknownKeyword(_) | TSType::TSNeverKeyword(_))
        });
        if !has_useless {
            return;
        }
        // `unknown &` as the leading operand is a deliberate TypeScript trick to
        // defer/distribute conditional-type evaluation over generic parameters,
        // not a no-op. Exempt it when used in those generic-aware contexts.
        let leads_with_unknown =
            matches!(intersection.types.first(), Some(TSType::TSUnknownKeyword(_)));
        if leads_with_unknown && is_deferral_trick(node, semantic) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, intersection.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Intersection with `unknown` or `never` is useless — simplify it.".into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

/// True when a `unknown &`-leading intersection sits in a context where the
/// `unknown &` prefix is the documented TypeScript trick to defer or distribute
/// type evaluation over generic parameters, rather than a no-op intersection:
///
/// - the check type of a conditional type (`unknown & T extends … ? … : …`), or
/// - the body of a generic type alias (`type A<P> = unknown & Foo<P>`).
fn is_deferral_trick<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let intersection_span = node.kind().span();
    let parent = semantic.nodes().parent_node(node.id());
    match parent.kind() {
        AstKind::TSConditionalType(conditional) => {
            conditional.check_type.span() == intersection_span
        }
        AstKind::TSTypeAliasDeclaration(alias) => {
            alias.type_parameters.is_some()
                && alias.type_annotation.span() == intersection_span
        }
        _ => false,
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

    #[test]
    fn flags_intersection_with_unknown() {
        assert_eq!(run_on("type X = Foo & unknown;").len(), 1);
    }

    #[test]
    fn flags_unknown_on_left() {
        assert_eq!(run_on("type X = unknown & Foo;").len(), 1);
    }

    #[test]
    fn flags_intersection_with_never() {
        assert_eq!(run_on("type X = Foo & never;").len(), 1);
    }

    #[test]
    fn allows_intersection_with_any() {
        assert!(run_on("type X = Foo & any;").is_empty());
    }

    #[test]
    fn allows_any_on_left() {
        assert!(run_on("type X = any & Foo;").is_empty());
    }

    #[test]
    fn allows_normal_intersection() {
        assert!(run_on("type X = Foo & Bar;").is_empty());
    }

    #[test]
    fn no_false_positive_on_any_prefix() {
        assert!(run_on("type X = anything & Foo;").is_empty());
    }

    #[test]
    fn allows_unknown_prefix_on_conditional_check_type() {
        let src = "export type UseSpringProps<Props extends object = any> = unknown &\n  PickAnimated<Props> extends infer State\n  ? State extends Lookup\n    ? Remap<ControllerUpdate<State> & { ref?: SpringRef<State> }>\n    : never\n  : never;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_unknown_prefix_in_generic_type_alias() {
        let src = "export type ControllerUpdate<\n  State extends Lookup = Lookup,\n  Item = undefined,\n> = unknown & ToProps<State> & ControllerProps<State, Item>;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_non_leading_unknown_in_generic_type_alias() {
        assert_eq!(run_on("type X<T> = Foo<T> & unknown;").len(), 1);
    }
}
