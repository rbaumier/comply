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
fn type_includes_undefined(ty: &TSType) -> bool {
    match ty {
        TSType::TSUndefinedKeyword(_) => true,
        TSType::TSVoidKeyword(_) => true,
        TSType::TSUnionType(union) => union.types.iter().any(type_includes_undefined),
        TSType::TSTypeReference(type_ref) => {
            let oxc_ast::ast::TSTypeName::IdentifierReference(id) = &type_ref.type_name else {
                return false;
            };
            if id.name.as_str() != "Promise" {
                return false;
            }
            let Some(params) = &type_ref.type_arguments else {
                return false;
            };
            params.params.iter().any(type_includes_undefined)
        }
        _ => false,
    }
}

/// Walk ancestors to find the enclosing function (`function`, arrow,
/// or method) and return whether its declared return type already
/// includes `undefined`. Returns `false` when no return type is
/// annotated — the rule keeps its original behaviour there.
fn enclosing_return_type_allows_undefined(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node_id) {
        let return_type = match ancestor.kind() {
            AstKind::Function(func) => func.return_type.as_ref(),
            AstKind::ArrowFunctionExpression(arrow) => arrow.return_type.as_ref(),
            _ => continue,
        };
        return return_type
            .map(|ann| type_includes_undefined(&ann.type_annotation))
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
}
