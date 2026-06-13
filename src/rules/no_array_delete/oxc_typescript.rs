//! no-array-delete oxc backend — flag `delete arr[i]` on array targets.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType, TSTypeName, TSTypeOperatorOperator};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UnaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["delete"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UnaryExpression(unary) = node.kind() else {
            return;
        };
        if unary.operator != oxc_ast::ast::UnaryOperator::Delete {
            return;
        }
        // Test files delete `process.env` keys and fixture entries in teardown —
        // bounded to the test scope with no non-mutating equivalent.
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        // The argument must be a computed member expression (bracket access).
        let Expression::ComputedMemberExpression(member) = &unary.argument else {
            return;
        };

        // Only fire with positive evidence the target is an array; deleting a
        // key from a plain object / record / dictionary is a valid operation
        // that creates no sparse hole.
        if !is_array_target(&member.object, &member.expression, semantic) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, unary.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`delete arr[i]` creates a sparse hole — use `arr.splice(i, 1)` instead."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when there is static evidence the deletion target is an array:
/// either the index is a numeric literal (`arr[0]`), or the target identifier
/// resolves to a binding declared as an array (literal `[...]`, `new Array(...)`,
/// or annotated `T[]` / tuple / `Array<T>` / `ReadonlyArray<T>` / `readonly T[]`).
fn is_array_target<'a>(
    object: &Expression<'a>,
    index: &Expression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    if matches!(index, Expression::NumericLiteral(_)) {
        return true;
    }
    let Expression::Identifier(id) = object else {
        return false;
    };
    let Some(ref_id) = id.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let AstKind::VariableDeclarator(decl) =
        semantic.nodes().kind(scoping.symbol_declaration(sym_id))
    else {
        return false;
    };
    if let Some(annotation) = &decl.type_annotation
        && is_array_type(&annotation.type_annotation)
    {
        return true;
    }
    matches!(&decl.init, Some(init) if is_array_initializer(init))
}

/// True for an initializer that produces an array: an array literal `[...]`
/// or `new Array(...)`.
fn is_array_initializer(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::ArrayExpression(_) => true,
        Expression::NewExpression(new_expr) => {
            matches!(&new_expr.callee, Expression::Identifier(id) if id.name == "Array")
        }
        _ => false,
    }
}

/// True for a TypeScript type that denotes an array or tuple:
/// `T[]`, `[A, B]`, `readonly T[]`, `Array<T>`, or `ReadonlyArray<T>`.
fn is_array_type(ty: &TSType<'_>) -> bool {
    match ty {
        TSType::TSArrayType(_) | TSType::TSTupleType(_) => true,
        TSType::TSTypeOperatorType(op) if op.operator == TSTypeOperatorOperator::Readonly => {
            is_array_type(&op.type_annotation)
        }
        TSType::TSTypeReference(reference) => matches!(
            &reference.type_name,
            TSTypeName::IdentifierReference(name)
                if matches!(name.name.as_str(), "Array" | "ReadonlyArray")
        ),
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
mod oxc_tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_in_test_file(src: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "t.tsx", crate::project::default_static_project_ctx(), &file)
    }

    #[test]
    fn flags_delete_array_element() {
        assert_eq!(run("delete arr[0];").len(), 1);
    }

    #[test]
    fn flags_delete_array_literal_binding() {
        let src = "const arr = [1, 2, 3]; delete arr[i];";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn flags_delete_array_typed_binding() {
        let src = "const arr: number[] = []; delete arr[i];";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn flags_delete_new_array_binding() {
        let src = "const arr = new Array(3); delete arr[i];";
        assert_eq!(run(src).len(), 1, "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_record_key_issue_1889() {
        // `plugins` is a record typed binding; the key is a `keyof`-typed param.
        let src = "type Plugins = Record<string, unknown>; const plugins: Plugins = {}; \
                   const clearPlugin = <K extends keyof Plugins>(pluginKey: K): void => { delete plugins[pluginKey]; };";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_descriptors_issue_1889() {
        // `descriptors` is a PropertyDescriptorMap from getOwnPropertyDescriptors.
        let src = "const descriptors = Object.getOwnPropertyDescriptors(base); delete descriptors[DRAFT_STATE as any];";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_member_target_issue_1889() {
        // `state.copy_` is a member access typed as the proxied object T.
        let src = "if (state.copy_) { delete state.copy_[prop]; }";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_object_typed_binding() {
        let src = "const obj: Record<string, number> = {}; delete obj[key];";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_object_literal_binding() {
        let src = "const obj = {}; delete obj[key];";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_delete_process_env_issue_479() {
        let src = "delete process.env[key];";
        assert!(run(src).is_empty(), "got {:?}", run(src));
    }

    #[test]
    fn skips_in_test_file_issue_582() {
        // Test teardown deletes fixture entries; bounded to test scope.
        assert!(run_in_test_file("delete fixtures[id];").is_empty());
    }
}
