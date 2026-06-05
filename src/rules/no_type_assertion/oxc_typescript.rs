//! no-type-assertion OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, name_is_generic_type_param_in_scope};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSArrayType, TSType, TSTypeName};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Returns `true` if the source line at `byte_offset` (or the immediately
/// preceding line) contains `// comply-ignore-reason: utility-type-constraint`.
///
/// This exempts single `as T` casts that work around deferred conditional
/// types in third-party libraries (e.g. Drizzle ORM's `TableLikeHasEmptySelection`)
/// where TypeScript cannot evaluate the conditional against a generic bound.
fn has_utility_type_constraint_comment(source: &str, byte_offset: usize) -> bool {
    const MARKER: &str = "// comply-ignore-reason: utility-type-constraint";
    let safe = byte_offset.min(source.len());
    let line_start = source[..safe].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = source[safe..]
        .find('\n')
        .map(|p| safe + p)
        .unwrap_or(source.len());
    if source[line_start..line_end].contains(MARKER) {
        return true;
    }
    if line_start > 0 {
        let prev_end = line_start - 1;
        let prev_start = source[..prev_end].rfind('\n').map(|p| p + 1).unwrap_or(0);
        if source[prev_start..prev_end].contains(MARKER) {
            return true;
        }
    }
    false
}

/// True when `ty` is a bare reference to a generic type parameter in scope
/// (e.g. `T` / `TFields`), with no type arguments of its own.
fn type_arg_is_generic_param(
    ty: &TSType,
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let TSType::TSTypeReference(r) = ty else { return false };
    if r.type_arguments.is_some() {
        return false;
    }
    let TSTypeName::IdentifierReference(id) = &r.type_name else { return false };
    name_is_generic_type_param_in_scope(id.name.as_str(), node_id, semantic)
}

fn peel_parens<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    let mut current = expr;
    while let Expression::ParenthesizedExpression(p) = current {
        current = &p.expression;
    }
    current
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSAsExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSAsExpression(as_expr) = node.kind() else { return };

        // Test files legitimately use assertions to build minimal stubs/mocks
        // and to satisfy the checker on intentionally-unreached paths.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // Allow `as const` — it's a type refinement, not a cast.
        let type_span = as_expr.type_annotation.span();
        let type_text = &ctx.source[type_span.start as usize..type_span.end as usize];
        if type_text == "const" {
            return;
        }

        // Allow `as never` and `as never[]` — explicit type-system escape
        // hatches used to bridge generic-table erasure (e.g. Drizzle's
        // `tx.insert(table).values(values as never[])`). These are not
        // narrowings; they're acknowledgements that the call site can't be
        // typed precisely against a generic library helper.
        if matches!(as_expr.type_annotation, TSType::TSNeverKeyword(_)) {
            return;
        }
        if let TSType::TSArrayType(arr) = &as_expr.type_annotation
            && let TSArrayType { element_type, .. } = &**arr
            && matches!(element_type, TSType::TSNeverKeyword(_))
        {
            return;
        }

        // Allow `as TParam` when `TParam` is a generic type parameter declared
        // on the enclosing function/method/class/interface/type-alias. These
        // are structural type-bridge casts (TanStack Router, generic wrappers,
        // etc.) — not narrowings, not escape hatches.
        if let TSType::TSTypeReference(r) = &as_expr.type_annotation
            && r.type_arguments.is_none()
        {
            let TSTypeName::IdentifierReference(id) = &r.type_name else { return };
            let name = id.name.as_str();
            if name_is_generic_type_param_in_scope(name, node.id(), semantic) {
                return;
            }
        }

        // Allow `expr as Foo<TParam>` where a type argument is an in-scope
        // generic type parameter (e.g. React Hook Form `field as Path<TFields>`).
        // `Path<TFields>` is a compile-time string-literal union derived from the
        // generic field shape — there is no runtime value to narrow it to, so the
        // cast is a structural type bridge, not a narrowing.
        if let TSType::TSTypeReference(r) = &as_expr.type_annotation
            && let Some(args) = &r.type_arguments
            && args
                .params
                .iter()
                .any(|t| type_arg_is_generic_param(t, node.id(), semantic))
        {
            return;
        }

        // Allow either half of an `x as unknown as T` chain — the chain is
        // the canonical contravariant-boundary escape hatch (matches the
        // `no-double-cast` skip). Without these two checks, the outer cast
        // and the inner `as unknown` still fire even though `no-double-cast`
        // correctly stays silent.
        //  - Outer half: `x as unknown as T` whose inner is `_ as unknown`.
        //    Peel any parentheses so `(x as unknown) as T` is treated the
        //    same as `x as unknown as T`.
        if let Expression::TSAsExpression(inner) = peel_parens(&as_expr.expression)
            && matches!(inner.type_annotation, TSType::TSUnknownKeyword(_))
        {
            return;
        }
        //  - Inner half: `_ as unknown` whose parent is another TSAsExpression.
        //    Walk past any ParenthesizedExpression parents so `(x as unknown)`
        //    inside a double-cast is still exempted.
        if matches!(as_expr.type_annotation, TSType::TSUnknownKeyword(_)) {
            let nodes = semantic.nodes();
            let mut cur = node.id();
            loop {
                let parent_id = nodes.parent_id(cur);
                if parent_id == cur {
                    break;
                }
                match nodes.kind(parent_id) {
                    AstKind::TSAsExpression(_) => return,
                    AstKind::ParenthesizedExpression(_) => {
                        cur = parent_id;
                    }
                    _ => break,
                }
            }
        }

        // Allow single `as T` when the line or the preceding line carries
        // `// comply-ignore-reason: utility-type-constraint` — an acknowledged
        // workaround for third-party deferred conditional types (e.g. Drizzle).
        if has_utility_type_constraint_comment(ctx.source, as_expr.span.start as usize) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Type assertion `as T` bypasses the type checker — use `satisfies`, type guards, or generics.".into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    fn run_in_test_file(source: &str) -> Vec<Diagnostic> {
        use crate::rules::file_ctx::{FileCtx, PathSegments};
        let file = FileCtx {
            path_segments: PathSegments { in_test_dir: true, ..Default::default() },
            ..Default::default()
        };
        crate::rules::test_helpers::run_oxc_tsx_with_file_ctx(source, &Check, &file)
    }

    #[test]
    fn flags_as_string() {
        let diags = run_on("const x = foo as string;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_assertions_in_test_files() {
        // Regression for issue #573: test stubs/mocks cast freely.
        assert!(run_in_test_file("const c = {} as AnyColumn;").is_empty());
        assert!(run_in_test_file("const e = vi.fn() as UseFormSetError<FieldValues>;").is_empty());
        // Regression for issue #793: tsd literal tuple assertions in test-d/ fixtures.
        assert!(run_in_test_file("const literal = ['foo'] as ['foo'];").is_empty());
    }

    #[test]
    fn allows_as_const() {
        assert!(run_on("const x = { a: 1 } as const;").is_empty());
    }

    #[test]
    fn allows_generic_type_param_in_function() {
        // Regression for #114: `as TSearch` where `<TSearch>` is on the
        // enclosing function is a structural type bridge, not a cast.
        let src = "function useTypedSearch<TSearch>(api: { useSearch: () => unknown }) {\n\
                   const search = api.useSearch() as TSearch;\n\
                   return search;\n}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_generic_type_param_in_arrow() {
        let src = "const f = <T>(x: unknown) => x as T;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_cast_to_generic_type_with_param_arg() {
        // Regression for #571: `field as Path<TFields>` (React Hook Form) bridges
        // a runtime string to a compile-time type-level union parameterized by an
        // in-scope generic param — no runtime narrowing exists.
        let src = "function setFieldError<T extends FieldValues>(setError: UseFormSetError<T>, field: string) {\n\
                   setError(field as Path<T>, { type: 'manual' });\n}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn flags_cast_to_generic_type_without_param_arg() {
        // `x as Path<string>` has no in-scope generic param — still a cast.
        let diags = run_on("function f() { return x as Path<string>; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_pascal_cased_when_not_generic_param() {
        // PascalCase looks like a generic param but isn't declared in scope.
        let diags = run_on("function f() { return x as MyType; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_as_never_array_for_drizzle_generic_table() {
        // Regression for #127: `as never[]` bridges generic-table erasure
        // when calling a generic Drizzle helper from a generic wrapper —
        // an explicit type-system escape hatch, not a narrowing.
        let src = "async function replaceRows<TRow>(tx: any, table: any, values: readonly TRow[]) {\n\
                   tx.insert(table).values(values as never[]);\n}";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_as_never() {
        // `as never` is the canonical "this branch is unreachable" /
        // generic-bridge escape hatch.
        let diags = run_on("const x = foo as never;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_outer_cast_of_as_unknown_as_chain() {
        // Regression for #114: the outer cast of `x as unknown as T` is part
        // of the canonical contravariant-boundary escape hatch and should
        // stay silent (mirrors the `no-double-cast` skip).
        let src = "const navigate = api.useNavigate() as unknown as \
                   (options: { search: (p: TSearch) => TSearch }) => unknown;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_drizzle_relations_filter_as_unknown_as_chain() {
        // Regression for #178: Drizzle relational types are invariant in
        // `TablesRelationalConfig`; structural relabel requires
        // `as unknown as <Type>`.
        let src = "type AnyRelationsFilter = unknown;\n\
                   declare const where: object;\n\
                   const widenedWhere = where as unknown as AnyRelationsFilter;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn flags_double_cast_without_unknown_middle() {
        // Negative: `x as any as Foo` is NOT the canonical escape hatch —
        // the middle must be `unknown` for the exemption to apply.
        let diags = run_on("const y = x as any as Foo;");
        assert!(!diags.is_empty(), "expected at least one diag");
    }

    #[test]
    fn allows_parenthesised_unknown_in_double_cast() {
        // Issue #178 follow-up — `(x as unknown) as Foo` is semantically
        // identical to `x as unknown as Foo`.
        let src = "declare const x: unknown; const y = (x as unknown) as Foo;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_triple_parenthesised_as_chain() {
        // `((x as A) as unknown) as B` — the middle isn't a plain `as unknown`
        // of the original value; the inner `as A` is the suspect cast. We
        // don't auto-exempt arbitrary triple casts.
        let src = "declare const x: unknown; const y = ((x as A) as unknown) as B;";
        let diags = run_on(src);
        assert!(!diags.is_empty(), "expected at least one diag for inner `as A` cast");
    }

    // Regression #388 — single `as T` with comply-ignore-reason: utility-type-constraint
    #[test]
    fn allows_utility_type_constraint_inline_comment() {
        // Drizzle deferred conditional type: `as AnyPgTable` with inline reason comment.
        let src = "const x = junctionTable as AnyPgTable; // comply-ignore-reason: utility-type-constraint";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_utility_type_constraint_preceding_comment() {
        // comply-ignore-reason on the preceding line.
        let src = "// comply-ignore-reason: utility-type-constraint\nconst x = junctionTable as AnyPgTable;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn still_flags_without_utility_type_constraint_comment() {
        // No comment → still flagged.
        let src = "const x = junctionTable as AnyPgTable;";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_utility_type_constraint_multiline_cast() {
        // Regression #388: multi-cast pattern from Drizzle query chain.
        let src = "// comply-ignore-reason: utility-type-constraint\n\
                   const cols = getColumns(refTable) as Record<string, PgColumn>;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }
}
