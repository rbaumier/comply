//! no-double-cast OXC backend — flag `x as unknown as T` style double casts.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType, TSTypeName};
use std::sync::Arc;

pub struct Check;

/// True when `ty` is the keyword `any`, `unknown`, or `never` — the top/bottom
/// pivots TypeScript itself requires for a forced cast when a direct `as T`
/// fails with TS2352. `any` is symmetric to `unknown` here: it is assignable
/// both from and to every type, so `x as any as T` is the same sanctioned
/// escape hatch as `x as unknown as T`.
fn is_escape_hatch_keyword(ty: &TSType) -> bool {
    matches!(
        ty,
        TSType::TSAnyKeyword(_) | TSType::TSUnknownKeyword(_) | TSType::TSNeverKeyword(_)
    )
}

/// True when the pivot type is an escape-hatch keyword directly, or a type
/// reference to a same-file alias that resolves to one. Theatre.js's
/// `type $IntentionalAny = any` is the canonical case: a named alias signals a
/// deliberate forced cast. Only locally-declared aliases resolve; an imported
/// or built-in name has no inspectable declaration and stays flagged
/// (precision over recall).
fn is_escape_hatch_pivot<'a>(ty: &TSType, semantic: &'a oxc_semantic::Semantic<'a>) -> bool {
    if is_escape_hatch_keyword(ty) {
        return true;
    }
    let TSType::TSTypeReference(type_ref) = ty else {
        return false;
    };
    let TSTypeName::IdentifierReference(ident) = &type_ref.type_name else {
        return false;
    };
    local_alias_resolves_to_escape_hatch(ident.name.as_str(), semantic)
}

/// Looks up same-file `type <name> = …` aliases and reports whether any
/// resolves to an escape-hatch keyword. Walks short alias chains
/// (`type A = any; type B = A;`) up to a small bound to avoid cycles.
fn local_alias_resolves_to_escape_hatch<'a>(
    name: &str,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let mut current = name.to_string();
    for _ in 0..8 {
        let mut next: Option<String> = None;
        for node in semantic.nodes().iter() {
            let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else {
                continue;
            };
            if alias.id.name.as_str() != current {
                continue;
            }
            if is_escape_hatch_keyword(&alias.type_annotation) {
                return true;
            }
            if let TSType::TSTypeReference(type_ref) = &alias.type_annotation
                && let TSTypeName::IdentifierReference(ident) = &type_ref.type_name
            {
                next = Some(ident.name.to_string());
            }
        }
        match next {
            Some(n) if n != current => current = n,
            _ => return false,
        }
    }
    false
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

        // Double casts are the standard pattern for test doubles / partial stubs.
        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // The inner expression of `x as A as B` is itself a TSAsExpression.
        let Expression::TSAsExpression(inner) = &as_expr.expression else {
            return;
        };

        // `x as any/unknown/never as T` are the canonical forced-cast pivots —
        // `any`/`unknown` (assignable from/to everything) and `never` (bottom
        // type) are what TypeScript itself requires when a direct `as T` fails
        // with TS2352. A same-file alias for one of them (Theatre.js's
        // `type $IntentionalAny = any`) signals the same deliberate bypass.
        // Flagging these produces noise on TanStack Router / kysely / library
        // bridges where the user has no other option.
        // The pivot is the INNER assertion's type (`A` in `x as A as B`).
        // Only skip if the inner cast's own expression is NOT itself a
        // TSAsExpression — a triple cast `((x as A) as unknown) as B` still
        // contains a real `as A as unknown` inner pair that should fire.
        if is_escape_hatch_pivot(&inner.type_annotation, semantic)
            && !matches!(inner.expression, Expression::TSAsExpression(_))
        {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, as_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Double cast `as X as Y` hides misaligned types. \
                      Fix the real problem: align the interface, or \
                      validate at the boundary with a type guard or Zod \
                      schema that actually checks the shape at runtime."
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_as_any_as_t() {
        // `as any as T` is symmetric to `as unknown as T`: `any` is assignable
        // both from and to every type, so it is the same sanctioned forced-cast
        // pivot, not a hidden misalignment.
        assert!(run_on("const x = value as any as User;").is_empty());
    }

    #[test]
    fn allows_as_alias_for_any_as_t() {
        // Theatre.js's `$IntentionalAny` pattern: a same-file alias that
        // resolves to `any` is a deliberate, named forced-cast pivot.
        let src = "type $IntentionalAny = any;\n\
                   const env = process.env as $IntentionalAny as Env;";
        assert!(run_on(src).is_empty(), "alias-to-any pivot should not flag");
    }

    #[test]
    fn allows_chained_alias_for_unknown_as_t() {
        let src = "type Top = unknown;\ntype Pivot = Top;\n\
                   const x = value as Pivot as User;";
        assert!(run_on(src).is_empty(), "chained alias-to-unknown should not flag");
    }

    #[test]
    fn flags_concrete_double_cast() {
        // A pivot through a concrete type is a genuine accidental double cast.
        assert_eq!(run_on("const x = value as Foo as Bar;").len(), 1);
    }

    #[test]
    fn flags_final_cast_to_any() {
        // The PIVOT is the inner assertion's type (`User`, concrete) — a final
        // cast TO `any` is not the forced-cast idiom and stays flagged.
        assert_eq!(run_on("const x = value as User as any;").len(), 1);
    }

    #[test]
    fn flags_alias_to_concrete_double_cast() {
        // An alias resolving to a concrete type is not an escape hatch.
        let src = "type Mid = Foo;\nconst x = value as Mid as Bar;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_triple_cast_through_concrete() {
        // `value as A as unknown as User`: the outer `... as User` fires because
        // its inner is itself an `as`-expression (the escape-hatch skip only
        // covers a real `_ as unknown` pivot, not a chain), and the middle
        // `... as unknown` fires because its pivot `A` is concrete.
        let src = "const x = value as A as unknown as User;";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_as_unknown_as_t() {
        // `as unknown as T` is the canonical contravariant-boundary escape
        // hatch — required by TanStack Router etc. for generic type bridging.
        let src = "const navigate = routeApi.useNavigate() as unknown as \
                   (options: { search: (p: TSearch) => TSearch }) => void;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_as_never_as_t() {
        // `as never as T` is the symmetric bottom-type forced-cast pivot —
        // TypeScript requires it (or `as unknown as T`) when a direct `as T`
        // fails with TS2352, so it is a deliberate idiom, not a hidden
        // misalignment.
        let src = "const x = value as never as User;";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_as_never_as_t_kysely_shape() {
        let src = "const [{ cid }] = (await this.executeQuery(\
                   CompiledQuery.raw(`select connection_id() as cid`))) \
                   as never as [{ cid: string }];";
        let diags = run_on(src);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_single_cast() {
        assert!(run_on("const x = value as MyType;").is_empty());
    }
}
