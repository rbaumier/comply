//! security-detect-non-literal-regexp oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, TSType, TSTypeAnnotation, TemplateLiteral, VariableDeclarationKind};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new RegExp"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };
        let Expression::Identifier(callee) = &new_expr.callee else {
            return;
        };
        if callee.name.as_str() != "RegExp" {
            return;
        }
        let Some(first_arg) = new_expr.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };
        if is_safe_pattern(expr, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new RegExp(<dynamic>)` lets user input drive the pattern — \
                      ReDoS / regex injection vector. Escape the input or use a \
                      static literal."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True when the `new RegExp(...)` source argument cannot carry attacker-controlled
/// string content, so the ReDoS / injection vector the rule targets is absent.
///
/// Safe shapes:
/// - A string/regexp literal, or a template literal whose `${}` slots are all
///   statically numeric (a number coerces to decimal digits only — no regex
///   metacharacters reachable).
/// - An identifier bound by a module/function-local `const` whose initializer is
///   itself a safe pattern (a constant pattern defined once, not runtime input).
fn is_safe_pattern(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    match expr {
        Expression::StringLiteral(_) | Expression::RegExpLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => template_slots_all_numeric(tpl, semantic),
        Expression::Identifier(ident) => is_const_literal_binding(ident, semantic),
        _ => false,
    }
}

/// True when every `${}` slot of `tpl` is a statically numeric expression, so the
/// interpolated text is digits only. A slot-free template is vacuously numeric.
fn template_slots_all_numeric(tpl: &TemplateLiteral, semantic: &oxc_semantic::Semantic) -> bool {
    tpl.expressions
        .iter()
        .all(|slot| is_static_numeric_expr(slot, semantic))
}

/// True when `expr` can only evaluate to a `number` — a numeric literal, or an
/// identifier whose declared type annotation is numeric. Such a value contributes
/// only decimal digits (or the fixed strings `Infinity`/`NaN`) to a pattern.
fn is_static_numeric_expr(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    match expr {
        Expression::NumericLiteral(_) => true,
        Expression::Identifier(ident) => binding_type_is_numeric(ident, semantic),
        _ => false,
    }
}

/// True when `ident` resolves to a `const` binding whose initializer is a safe
/// pattern (string/regexp literal, or numeric-only template literal). A `let`/`var`
/// binding can be reassigned, and a parameter is runtime input, so neither qualifies.
fn is_const_literal_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind as AK;

    let Some(decl) = resolve_binding_declarator(ident, semantic) else {
        return false;
    };
    let AK::VariableDeclarator(decl) = decl else {
        return false;
    };
    if decl.kind != VariableDeclarationKind::Const {
        return false;
    }
    let Some(init) = &decl.init else {
        return false;
    };
    is_safe_pattern(init, semantic)
}

/// True when `ident` resolves to a binding (parameter or variable declarator) whose
/// type annotation is numeric — `number`, or a union all of whose members are
/// numeric / `undefined` / `null` (e.g. `number | undefined`). Without an annotation
/// the type is unknown and the value is treated as potentially string-bearing.
fn binding_type_is_numeric(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind as AK;

    let annotation = match resolve_binding_declarator(ident, semantic) {
        Some(AK::FormalParameter(param)) => param.type_annotation.as_deref(),
        Some(AK::VariableDeclarator(decl)) => decl.type_annotation.as_deref(),
        _ => None,
    };
    annotation.is_some_and(type_annotation_is_numeric)
}

/// True when the type annotation denotes only numbers (allowing `undefined`/`null`
/// union members, which cannot contribute string content either).
fn type_annotation_is_numeric(annotation: &TSTypeAnnotation) -> bool {
    type_is_numeric(&annotation.type_annotation)
}

fn type_is_numeric(ty: &TSType) -> bool {
    match ty {
        TSType::TSNumberKeyword(_) => true,
        TSType::TSParenthesizedType(paren) => type_is_numeric(&paren.type_annotation),
        TSType::TSUnionType(union) => union.types.iter().all(|member| {
            matches!(
                member,
                TSType::TSNumberKeyword(_)
                    | TSType::TSUndefinedKeyword(_)
                    | TSType::TSNullKeyword(_)
            ) || type_is_numeric(member)
        }),
        _ => false,
    }
}

/// Resolves an identifier reference to the AST node that declares its binding
/// (a `FormalParameter` or `VariableDeclarator`), via reference → symbol →
/// declaration node. Returns `None` for unresolved references (imports, globals).
fn resolve_binding_declarator<'a>(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> Option<AstKind<'a>> {
    use oxc_ast::AstKind as AK;

    let ref_id = ident.reference_id.get()?;
    let scoping = semantic.scoping();
    let sym_id = scoping.get_reference(ref_id).symbol_id()?;
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
        .find(|kind| matches!(kind, AK::FormalParameter(_) | AK::VariableDeclarator(_)))
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
    fn flags_dynamic_regexp() {
        let src = r#"const r = new RegExp(userInput);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_static_regexp() {
        let src = r#"const r = new RegExp("^foo$");"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_numeric_param_in_template() {
        // Issue #3374 pattern 1: number param interpolated into a template literal.
        let src = r#"
            export const uuid = (version?: number | undefined): RegExp => {
              return new RegExp(`^([0-9a-fA-F]{8}-${version}[0-9a-fA-F]{3})$`);
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_module_const_string_pattern() {
        // Issue #3374 pattern 2: module-level const string passed to new RegExp.
        let src = r#"
            const _emoji: string = `^(\\p{Extended_Pictographic}|\\p{Emoji_Component})+$`;
            export function emoji(): RegExp {
              return new RegExp(_emoji, "u");
            }
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_runtime_string_in_template() {
        // A string-typed value interpolated into the template can carry metacharacters.
        let src = r#"
            function build(input: string): RegExp {
              return new RegExp(`^${input}$`);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_user_derived_string() {
        // Over-exemption guard: a genuinely runtime/user-derived string still flags.
        let src = r#"const r = new RegExp(req.query.pattern);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_let_string_binding() {
        // A `let` binding can be reassigned to runtime input — not constant.
        let src = r#"
            let pattern = "^foo$";
            pattern = req.query.p;
            const r = new RegExp(pattern);
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
