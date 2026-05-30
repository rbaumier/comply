//! unicorn-no-useless-undefined oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType};
use std::sync::Arc;

pub struct Check;

fn is_undefined_identifier(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(id) if id.name.as_str() == "undefined")
}

/// True if `ty` is (or contains as a union member) `undefined` or `void`.
/// `void` is included because TypeScript allows `return undefined` in `void`
/// functions, and `consistent-return`/`require-explicit-undefined` may require
/// it. Also unwraps a single `Promise<T>` layer so that async functions
/// declared as `Promise<T | undefined>` are recognised.
/// Named type references (non-Promise) are resolved as type aliases via
/// `semantic` — so `type Foo = Bar | undefined; function f(): Foo` is
/// also recognised.
fn type_includes_undefined<'a>(ty: &TSType<'a>, semantic: &oxc_semantic::Semantic<'a>) -> bool {
    match ty {
        TSType::TSUndefinedKeyword(_) => true,
        TSType::TSVoidKeyword(_) => true,
        TSType::TSUnionType(union) => {
            union.types.iter().any(|t| type_includes_undefined(t, semantic))
        }
        TSType::TSTypeReference(type_ref) => {
            let oxc_ast::ast::TSTypeName::IdentifierReference(id) = &type_ref.type_name else {
                return false;
            };
            let name = id.name.as_str();
            if name == "Promise" {
                let Some(params) = &type_ref.type_arguments else {
                    return false;
                };
                return params
                    .params
                    .iter()
                    .any(|t| type_includes_undefined(t, semantic));
            }
            // Resolve the name as a type alias declaration in this file.
            resolve_alias_includes_undefined(name, semantic)
        }
        _ => false,
    }
}

/// Scan all nodes for a `type Name = ...` declaration and check whether
/// its RHS includes `undefined`. Returns `false` when no matching alias
/// is found (e.g. the type comes from an import — we can't inspect it).
fn resolve_alias_includes_undefined<'a>(
    name: &str,
    semantic: &oxc_semantic::Semantic<'a>,
) -> bool {
    for node in semantic.nodes().iter() {
        let AstKind::TSTypeAliasDeclaration(alias) = node.kind() else {
            continue;
        };
        if alias.id.name.as_str() == name {
            return type_includes_undefined(&alias.type_annotation, semantic);
        }
    }
    false
}

/// Walk ancestors to find the enclosing function (`function`, arrow,
/// or method) and return whether its declared return type already
/// includes `undefined`. Returns `false` when no return type is
/// annotated — the rule keeps its original behaviour there.
fn enclosing_return_type_allows_undefined<'a>(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node_id) {
        let return_type = match ancestor.kind() {
            AstKind::Function(func) => func.return_type.as_ref(),
            AstKind::ArrowFunctionExpression(arrow) => arrow.return_type.as_ref(),
            _ => continue,
        };
        return return_type
            .map(|ann| type_includes_undefined(&ann.type_annotation, semantic))
            .unwrap_or(false);
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ReturnStatement, AstType::VariableDeclarator]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["undefined"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ReturnStatement(ret) => {
                let Some(arg) = &ret.argument else { return };
                if !is_undefined_identifier(arg) {
                    return;
                }
                if enclosing_return_type_allows_undefined(node.id(), semantic) {
                    return;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, ret.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`return undefined` is redundant — drop the `undefined` \
                              and let the implicit return take over."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = &decl.init else { return };
                if !is_undefined_identifier(init) {
                    return;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Explicit `= undefined` is redundant — `let x;` is already \
                              undefined."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_return_undefined() {
        let src = "function f() { return undefined; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_let_assigned_undefined() {
        let src = "let x = undefined;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_bare_return() {
        let src = "function f() { return; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_uninitialised_let() {
        let src = "let x;";
        assert!(run(src).is_empty());
    }

    /// Regression for #149 — when the enclosing function declares
    /// `T | undefined` as its return type, the explicit
    /// `return undefined;` literally matches the annotation and is the
    /// only shape that also satisfies `require-explicit-undefined` and
    /// `consistent-return`. Do not flag.
    #[test]
    fn allows_return_undefined_when_return_type_includes_undefined() {
        let src = "
            type DefinedUsersWhere = { id: string };
            function multiLevelFilterUsers(levels: number[]): DefinedUsersWhere | undefined {
                const [first] = levels;
                if (first === undefined) {
                    return undefined;
                }
                return { id: String(first) };
            }
        ";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_return_undefined_in_async_promise_union() {
        let src = "
            async function load(): Promise<string | undefined> {
                return undefined;
            }
        ";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_return_undefined_in_arrow_with_union_return_type() {
        let src = "const f = (): number | undefined => { return undefined; };";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_return_undefined_when_return_type_excludes_undefined() {
        let src = "function f(): string { return undefined; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_return_undefined_without_return_type_annotation() {
        let src = "function f() { return undefined; }";
        assert_eq!(run(src).len(), 1);
    }

    /// Regression for #373 — `void | undefined` is the explicit-undefined
    /// pattern required by `consistent-return`/`require-explicit-undefined`.
    /// The union contains `undefined` so must not be flagged.
    #[test]
    fn allows_return_undefined_when_return_type_is_void_or_undefined() {
        let src = "
            function handler(): void | undefined {
                if (Math.random() > 0.5) {
                    return undefined;
                }
            }
        ";
        assert!(run(src).is_empty());
    }

    /// Regression for #373 — a plain `void` return type also means the
    /// function may legitimately write `return undefined` to satisfy
    /// `consistent-return` / `require-explicit-undefined`. Must not flag.
    #[test]
    fn allows_return_undefined_when_return_type_is_void() {
        let src = "
            function handler(): void {
                if (Math.random() > 0.5) {
                    return undefined;
                }
            }
        ";
        assert!(run(src).is_empty());
    }

    /// Regression for #563 — return type is a type alias that resolves to
    /// `T | undefined`. oxlint's `require-explicit-undefined` forces
    /// `return undefined;` in functions with a non-void return type, so
    /// comply must not flag it when the alias includes `undefined`.
    #[test]
    fn allows_return_undefined_when_return_type_is_alias_for_union_with_undefined() {
        let src = "
            type WhereClause = { sql: string; params: unknown[] };
            type WhereResult = WhereClause | undefined;
            function viewableTeamsWhere(scope: string): WhereResult {
                switch (scope) {
                    case 'all': return { sql: 'true', params: [] };
                    case 'own': return { sql: 'member = $1', params: ['u'] };
                    default: return undefined;
                }
            }
        ";
        assert!(run(src).is_empty());
    }

    /// Regression for #563 — same as above but with an arrow function
    /// (matches the `customError` pattern from zod-i18n.ts in amadeo).
    #[test]
    fn allows_return_undefined_in_arrow_when_return_type_is_alias_for_union_with_undefined() {
        let src = "
            type DateOriginError = string | undefined;
            const customError = (origin: string): DateOriginError => {
                if (origin === 'custom') return 'custom error';
                if (origin === 'relative') return 'relative error';
                return undefined;
            };
        ";
        assert!(run(src).is_empty());
    }

    /// Non-regression: a type alias that does NOT include `undefined` must
    /// still be flagged.
    #[test]
    fn still_flags_return_undefined_when_alias_excludes_undefined() {
        let src = "
            type Name = string;
            function f(): Name { return undefined; }
        ";
        assert_eq!(run(src).len(), 1);
    }
}
