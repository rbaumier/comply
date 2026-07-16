//! security-detect-non-literal-regexp oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    ArrayExpressionElement, BindingPattern, Expression, ObjectPropertyKind, Statement, TSType,
    TSTypeAnnotation, TemplateLiteral, VariableDeclarationKind,
};
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
/// - A string/regexp literal, or a template literal whose `${}` slots are each
///   themselves safe — a statically numeric expression (digits only) or any other
///   safe pattern (string literal, const literal binding, const-keys join, …).
/// - An identifier bound by a module/function-local `const` whose initializer is
///   itself a safe pattern (a constant pattern defined once, not runtime input).
/// - A `.source` member access whose object is itself a safe pattern, i.e.
///   `new RegExp(RE.source, flags)` rebuilding a regex from a const literal regex
///   with different flags — `.source` reads the compile-time pattern text.
/// - An `Object.keys(<const-object>).join(<string literal>)` chain — a local `const`
///   object literal with statically-written keys (no computed `[expr]` key, no
///   `...spread`) has author-fixed keys, so the joined string is fixed at author
///   time, not runtime input.
/// - A `<const-string-array>.join(<string literal>)` or
///   `<const-string-array>.map(<safe callback>).join(<string literal>)` chain — a
///   local `const` array literal whose every element is a string literal has
///   author-fixed elements, so joining them (optionally after mapping each through a
///   callback whose only free value is the element) yields a string fixed at author
///   time.
fn is_safe_pattern(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    match expr {
        Expression::StringLiteral(_) | Expression::RegExpLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => template_slots_all_safe(tpl, semantic),
        Expression::Identifier(ident) => is_const_literal_binding(ident, semantic),
        // `RE.source` is safe only when the object is itself a safe pattern (a const
        // bound to a regex/string literal), so the read yields a compile-time-fixed
        // string. `.source` on a non-const or call-expression object (e.g.
        // `buildPattern().source`) is not provably static and stays flagged.
        Expression::StaticMemberExpression(member) if member.property.name.as_str() == "source" => {
            is_safe_pattern(&member.object, semantic)
        }
        Expression::CallExpression(call) => {
            is_object_keys_join_of_const(call, semantic)
                || is_const_string_array_join(call, semantic)
        }
        _ => false,
    }
}

/// True when every `${}` slot of `tpl` is provably developer-controlled: a
/// statically numeric expression (digits only) or any other safe pattern. A
/// slot-free template is vacuously safe.
fn template_slots_all_safe(tpl: &TemplateLiteral, semantic: &oxc_semantic::Semantic) -> bool {
    tpl.expressions
        .iter()
        .all(|slot| is_static_numeric_expr(slot, semantic) || is_safe_pattern(slot, semantic))
}

/// True when `call` is the chain `Object.keys(<const-object>).join(<string literal>)`.
/// The keys of a locally-declared `const` object literal are fixed by the author, so
/// joining them with a literal separator yields a developer-controlled string — no
/// attacker input can reach the resulting regex pattern.
fn is_object_keys_join_of_const(
    call: &oxc_ast::ast::CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    // Outer call must be `<obj>.join(<StringLiteral>)`.
    let Expression::StaticMemberExpression(join_member) = &call.callee else {
        return false;
    };
    if join_member.property.name.as_str() != "join" {
        return false;
    }
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(Expression::StringLiteral(_)) =
        call.arguments.first().and_then(|arg| arg.as_expression())
    else {
        return false;
    };
    // `<obj>` must itself be `Object.keys(<const-object>)`.
    let Expression::CallExpression(keys_call) = &join_member.object else {
        return false;
    };
    is_object_keys_of_const(keys_call, semantic)
}

/// True when `call` is `Object.keys(<ident>)` and `<ident>` resolves to a local
/// `const` binding whose initializer is an object literal with statically-written
/// keys (no computed `[expr]` key, no `...spread`). Such keys are fixed at author
/// time; a `let`/`var`, parameter, imported object, or an object whose keys can be
/// runtime-derived (computed/spread) is not provably static.
fn is_object_keys_of_const(
    call: &oxc_ast::ast::CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    use oxc_ast::AstKind as AK;

    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != "keys" {
        return false;
    }
    let Expression::Identifier(obj_ident) = &member.object else {
        return false;
    };
    if obj_ident.name.as_str() != "Object" {
        return false;
    }
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(Expression::Identifier(arg_ident)) =
        call.arguments.first().and_then(|arg| arg.as_expression())
    else {
        return false;
    };
    let Some(AK::VariableDeclarator(decl)) = resolve_binding_declarator(arg_ident, semantic) else {
        return false;
    };
    if decl.kind != VariableDeclarationKind::Const {
        return false;
    }
    let Some(Expression::ObjectExpression(obj)) = &decl.init else {
        return false;
    };
    // Every key must be fixed by the author. A computed key (`[expr]: v`) or a
    // spread (`...src`) can pull a runtime-derived string into `Object.keys`, so
    // such an object's keys are not provably developer-controlled.
    obj.properties
        .iter()
        .all(|prop| matches!(prop, ObjectPropertyKind::ObjectProperty(p) if !p.computed))
}

/// True when `call` is `<arr>.join(<string literal>)` or
/// `<arr>.map(<callback>).join(<string literal>)`, where `<arr>` is a local `const`
/// array literal (or an inline array literal) whose every element is a string
/// literal. Joining author-fixed string literals with a literal separator yields a
/// string fixed at author time — no attacker input can reach the resulting pattern.
/// For the `.map` variant the callback body must itself be a safe pattern that treats
/// its element parameter (bound to those string literals) as a safe leaf.
fn is_const_string_array_join(
    call: &oxc_ast::ast::CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    // Outer call must be `<receiver>.join(<StringLiteral>)`.
    let Expression::StaticMemberExpression(join_member) = &call.callee else {
        return false;
    };
    if join_member.property.name.as_str() != "join" {
        return false;
    }
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(Expression::StringLiteral(_)) =
        call.arguments.first().and_then(|arg| arg.as_expression())
    else {
        return false;
    };
    // `<receiver>` is either `<arr>.map(<callback>)` or `<arr>` directly.
    match &join_member.object {
        Expression::CallExpression(map_call) => is_const_string_array_map(map_call, semantic),
        receiver => is_const_string_literal_array(receiver, semantic),
    }
}

/// True when `call` is `<arr>.map(<arrow>)` where `<arr>` is a const string-literal
/// array and `<arrow>` is an expression-bodied arrow whose body is a safe pattern,
/// treating the arrow's first (element) parameter as a safe leaf. A block-bodied
/// arrow, a non-arrow callback, or a destructured element parameter is not provably
/// safe and stays flagged.
fn is_const_string_array_map(
    call: &oxc_ast::ast::CallExpression,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let Expression::StaticMemberExpression(map_member) = &call.callee else {
        return false;
    };
    if map_member.property.name.as_str() != "map" {
        return false;
    }
    if !is_const_string_literal_array(&map_member.object, semantic) {
        return false;
    }
    if call.arguments.len() != 1 {
        return false;
    }
    let Some(Expression::ArrowFunctionExpression(arrow)) =
        call.arguments.first().and_then(|arg| arg.as_expression())
    else {
        return false;
    };
    if !arrow.expression {
        return false;
    }
    let Some(BindingPattern::BindingIdentifier(param)) =
        arrow.params.items.first().map(|item| &item.pattern)
    else {
        return false;
    };
    let Some(Statement::ExpressionStatement(body)) = arrow.body.statements.first() else {
        return false;
    };
    is_safe_pattern_with_param(&body.expression, param.name.as_str(), semantic)
}

/// True when `expr` is an inline array literal, or an identifier resolving to a local
/// `const` binding initialized to an array literal, whose every element is a string
/// literal. A `let`/`var`, parameter, imported binding, non-array initializer, or an
/// array with a spread, a hole, or any non-string-literal element is not provably
/// author-fixed.
fn is_const_string_literal_array(expr: &Expression, semantic: &oxc_semantic::Semantic) -> bool {
    use oxc_ast::AstKind as AK;

    let array = match expr {
        Expression::ArrayExpression(array) => array,
        Expression::Identifier(ident) => {
            let Some(AK::VariableDeclarator(decl)) = resolve_binding_declarator(ident, semantic)
            else {
                return false;
            };
            if decl.kind != VariableDeclarationKind::Const {
                return false;
            }
            let Some(Expression::ArrayExpression(array)) = &decl.init else {
                return false;
            };
            array
        }
        _ => return false,
    };
    array
        .elements
        .iter()
        .all(|el| matches!(el, ArrayExpressionElement::StringLiteral(_)))
}

/// True when `expr` is a safe pattern, additionally treating a bare reference to
/// `element_param` — the `.map` callback's element parameter, bound to the const
/// array's string-literal elements — as a safe leaf. Every other interpolation slot
/// must independently satisfy `is_safe_pattern`, so a captured outer variable of
/// unknown provenance is not exempted.
fn is_safe_pattern_with_param(
    expr: &Expression,
    element_param: &str,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    match expr {
        Expression::Identifier(ident) if ident.name.as_str() == element_param => true,
        Expression::TemplateLiteral(tpl) => tpl.expressions.iter().all(|slot| {
            is_static_numeric_expr(slot, semantic)
                || is_safe_pattern_with_param(slot, element_param, semantic)
        }),
        _ => is_safe_pattern(expr, semantic),
    }
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
    fn skips_dynamic_regexp_in_test_file() {
        // Regression for rbaumier/comply#6059 — `new RegExp(f.exception)` used as
        // the error-matcher argument to `assert.throws()` in a `.spec.ts` test.
        // The pattern is fixture-derived and never reaches a running service, so
        // the ReDoS / injection vector is absent. Production-only harm → the
        // central `skip_in_test_dir` gate suppresses it in test files.
        let src = r#"
            fixtures.invalid.forEach(f => {
              it('throws', () => {
                assert.throws(() => baddress.fromBase58Check(f.address),
                  new RegExp(f.address + ' ' + f.exception));
              });
            });
        "#;
        let diags =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "test/address.spec.ts");
        assert!(diags.is_empty());
    }

    #[test]
    fn still_flags_dynamic_regexp_in_production_source() {
        // The test-dir skip is scoped to test files only — a dynamic regex in
        // production source can still be driven by attacker input and is flagged.
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "const r = new RegExp(req.query.pattern);",
            "src/router.ts",
        );
        assert_eq!(diags.len(), 1);
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

    #[test]
    fn allows_const_regex_literal_source() {
        // Issue #6282: rebuilding a regex from a const regex literal with new flags.
        // `RE.source` reads the compile-time pattern, not attacker input.
        let src = r#"
            const HEAD_SSR_FILTER_RE = /\bhead\.ssr\b/;
            const HEAD_SSR_RE = new RegExp(HEAD_SSR_FILTER_RE.source, 'g');
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_source_on_call_expression() {
        // Tightness guard: `.source` on a call-expression object is not a const
        // regex literal, so the pattern is not provably static — still flagged.
        let src = r#"const r = new RegExp(buildPattern().source);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_object_keys_join_of_const_object_in_template() {
        // Issue #6901: `Object.keys(CONST_OBJ).join(literal)` in a template slot is a
        // fixed, developer-controlled string — the regex character class is static.
        let src = r#"
            const replacements = {
                ' ': '\\u2028',
                ' ': '\\u2029'
            };
            const pattern = new RegExp(`[${Object.keys(replacements).join('')}]`, 'g');
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_object_keys_join_of_const_object_direct() {
        // The same chain as the first RegExp argument, with no template wrapper.
        let src = r#"
            const replacements = { a: 1, b: 2 };
            const pattern = new RegExp(Object.keys(replacements).join('|'));
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_object_keys_join_of_let_binding() {
        // A `let` binding can be reassigned to runtime input — keys not provably static.
        let src = r#"
            let replacements = { a: 1 };
            const pattern = new RegExp(`[${Object.keys(replacements).join('')}]`, 'g');
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_object_keys_join_of_param() {
        // A function parameter is runtime input — its keys can carry metacharacters.
        let src = r#"
            function build(replacements: Record<string, string>): RegExp {
                return new RegExp(`[${Object.keys(replacements).join('')}]`, 'g');
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_object_keys_join_of_imported_object() {
        // An imported object is not a local const literal — its keys are not provably
        // developer-controlled in this module.
        let src = r#"
            import { replacements } from './config';
            const pattern = new RegExp(`[${Object.keys(replacements).join('')}]`, 'g');
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_object_keys_join_with_dynamic_separator() {
        // The separator is a variable, not a string literal — the joined string is
        // not provably static.
        let src = r#"
            const replacements = { a: 1 };
            const pattern = new RegExp(`[${Object.keys(replacements).join(sep)}]`, 'g');
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_object_keys_join_of_const_with_computed_key() {
        // A computed key can be runtime-derived — `Object.keys` then yields the
        // attacker's string. The const object's keys are not provably static.
        let src = r#"
            const replacements = { [req.query.evil]: 1 };
            const pattern = new RegExp(Object.keys(replacements).join('|'));
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_object_keys_join_of_const_with_spread() {
        // A spread pulls runtime keys into the object — keys not provably static.
        let src = r#"
            const replacements = { ...req.query };
            const pattern = new RegExp(`[${Object.keys(replacements).join('')}]`, 'g');
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_module_const_string_template_slot() {
        // Issue #6888: a module-level `const` string literal used as a template slot
        // is the same developer-controlled value as the already-exempt direct form
        // `new RegExp(MIDDLEWARE_MODULE_ID)` — template wrapping does not change safety.
        let src = r#"
            export const MIDDLEWARE_MODULE_ID = 'virtual:astro:middleware';
            const r = new RegExp(`^${MIDDLEWARE_MODULE_ID}$`);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_numeric_literal_template_slot() {
        // A numeric-literal slot contributes only digits — no metacharacters reach
        // the pattern, so the template stays safe.
        let src = r#"const r = new RegExp(`^${42}$`);"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_let_binding_template_slot() {
        // A `let` binding can be reassigned to runtime input — its value is not a
        // provably-static const, so the template slot is flagged.
        let src = r#"
            let v = getId();
            const r = new RegExp(`^${v}$`);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_call_expression_template_slot() {
        // A call-expression slot returns a runtime value that can carry
        // metacharacters — still flagged.
        let src = r#"const r = new RegExp(`^${getStr()}$`);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_const_string_array_map_join_in_template() {
        // Issue #7617: a const array of string literals mapped through an arrow that
        // returns a template built only from the element, then joined with a literal
        // separator, is a string fixed at author time — no runtime input reaches it.
        let src = r#"
            export const isMaybeMermaidDefinition = (text) => {
              const chartTypes = ["flowchart", "graph", "sequenceDiagram", "block"];
              const re = new RegExp(
                `^(?:%%{.*?}%%[\\s\\n]*)?\\b(?:${chartTypes
                  .map((x) => `\\s*${x}(-beta)?`)
                  .join("|")})\\b`,
              );
              return re.test(text);
            };
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_const_string_array_join_direct() {
        // The simpler `<const-string-array>.join(<literal>)` chain with no `.map`.
        let src = r#"
            const arr = ["a", "b", "c"];
            const r = new RegExp(arr.join("|"));
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inline_string_array_map_join() {
        // An inline array literal receiver is as author-fixed as a const binding.
        let src = r#"const r = new RegExp(["a", "b"].map((x) => `${x}?`).join("|"));"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_escape_regexp_of_user_input() {
        // A sanitizer call is not a const-array join chain — recognizing it by name
        // would be a name allowlist. It stays flagged.
        let src = r#"const r = new RegExp(escapeRegExp(userInput));"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_join_of_let_array() {
        // A `let` array can be reassigned to runtime input — not provably fixed.
        let src = r#"
            let arr = ["a", "b"];
            arr = req.query.list;
            const r = new RegExp(arr.map((x) => `${x}`).join("|"));
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_join_of_array_with_non_literal_element() {
        // An array element that is not a string literal can carry runtime input.
        let src = r#"
            const arr = ["a", req.query.b];
            const r = new RegExp(arr.map((x) => `${x}`).join("|"));
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_join_of_param_array() {
        // A function parameter is runtime input — its elements can carry metacharacters.
        let src = r#"
            function build(arr) {
              return new RegExp(arr.map((x) => `${x}`).join("|"));
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_join_of_const_array_with_spread() {
        // A spread pulls runtime elements into the array — not provably author-fixed.
        let src = r#"
            const arr = [...req.query.list, "b"];
            const r = new RegExp(arr.join("|"));
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_map_callback_interpolates_outer_variable() {
        // The callback interpolates a captured outer variable of runtime provenance,
        // not just its bound element — the joined string is not provably fixed.
        let src = r#"
            const arr = ["a", "b"];
            const suffix = getInput();
            const r = new RegExp(arr.map((x) => `${x}${suffix}`).join("|"));
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
