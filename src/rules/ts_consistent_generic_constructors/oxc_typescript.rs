use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, TSType};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclarator(declarator) = node.kind() else {
            return;
        };
        // Must have a `new Foo()` init.
        let Some(init) = &declarator.init else { return };
        let Expression::NewExpression(new_expr) = init else {
            return;
        };
        // The new expression must NOT have type arguments.
        if new_expr.type_arguments.is_some() {
            return;
        }
        // Must have a type annotation with type arguments on a TSTypeReference.
        let Some(type_ann) = &declarator.type_annotation else {
            return;
        };
        let TSType::TSTypeReference(ref_ty) = &type_ann.type_annotation else {
            return;
        };
        if ref_ty.type_arguments.is_none() {
            return;
        }
        // Verify constructor name matches type name. For namespace-qualified
        // annotations (`Take.Take`) and member-expression constructors
        // (`tArray.TArrayImpl`), compare the trailing name segment: an interface
        // annotation instantiated by a differently-named concrete class is not
        // the same type, so the generic arguments cannot be moved to the
        // constructor.
        let constructor_name = match &new_expr.callee {
            Expression::Identifier(id) => Some(id.name.as_str()),
            Expression::StaticMemberExpression(m) => {
                Some(m.property.name.as_str())
            }
            _ => None,
        };
        let type_name = match &ref_ty.type_name {
            oxc_ast::ast::TSTypeName::IdentifierReference(ident) => {
                Some(ident.name.as_str())
            }
            oxc_ast::ast::TSTypeName::QualifiedName(q) => {
                Some(q.right.name.as_str())
            }
            _ => None,
        };
        if let (Some(cn), Some(tn)) = (constructor_name, type_name)
            && cn != tn {
                return;
            }
        let BindingPattern::BindingIdentifier(id) = &declarator.id else {
            return;
        };
        let (line, column) =
            byte_offset_to_line_col(ctx.source, id.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Generic type arguments should be specified on the constructor, not the type annotation.".into(),
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
    use crate::rules::test_helpers::run_rule_gated;

    #[test]
    fn skips_qualified_annotation_with_concrete_class_constructor() {
        // Issue #7173 — `Take.Take<never>` is an interface instantiated by the
        // concrete class `TakeImpl`; the trailing names differ, so the generic
        // args cannot be moved to the constructor.
        let src = "const end: Take.Take<never> = new TakeImpl(Exit.fail());";
        let diags = run_rule_gated(&Check, src, "src/internal/take.ts");
        assert!(diags.is_empty(), "qualified annotation vs concrete class must not fire, got: {diags:?}");
    }

    #[test]
    fn skips_member_constructor_with_qualified_annotation() {
        // Member-expression constructor (`tArray.TArrayImpl`) against a
        // namespace-qualified annotation (`TArray.TArray`); trailing names differ.
        let src = "const a: TArray.TArray<X> = new tArray.TArrayImpl(y);";
        let diags = run_rule_gated(&Check, src, "src/internal/stm/tArray.ts");
        assert!(diags.is_empty(), "member constructor vs qualified annotation must not fire, got: {diags:?}");
    }

    #[test]
    fn flags_same_bare_name_both_sides() {
        // Negative space: same type on both sides — the generic args belong on
        // the constructor, so this must still fire.
        let src = "const m: Map<K, V> = new Map();";
        let diags = run_rule_gated(&Check, src, "src/store.ts");
        assert_eq!(diags.len(), 1, "same bare name both sides must fire, got: {diags:?}");
    }

    #[test]
    fn flags_qualified_pair_with_same_trailing_name() {
        // Negative space: qualified on both sides with the same trailing segment
        // denotes the same type, so it must still fire.
        let src = "const m: ns.Map<K, V> = new ns.Map();";
        let diags = run_rule_gated(&Check, src, "src/store.ts");
        assert_eq!(diags.len(), 1, "qualified pair with same trailing name must fire, got: {diags:?}");
    }
}
