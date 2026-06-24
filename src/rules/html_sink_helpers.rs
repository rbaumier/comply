//! Shared exemptions for the HTML-sink rules (`no-inner-html`,
//! `no-unsanitized-property`, `no-dynamic-template`).
//!
//! A template literal whose every `${...}` interpolation is provably numeric
//! cannot carry HTML markup — a number serializes to digits, never a `<tag>`.
//! The canonical case is a CSS overlay built from pixel coordinates
//! (`` `left: ${x}px` ``). Any string-typed or unknown interpolation keeps the
//! assignment flagged.
//!
//! Setting `.innerHTML` on a `<template>` element is not an XSS sink: a
//! template's content is an inert, off-document `DocumentFragment` whose scripts
//! never execute and which is not part of the live DOM. This is the standard
//! safe HTML-parsing idiom. [`lhs_object_is_template_element`] proves the
//! assignment target is a `<template>` so only that element type is exempt —
//! `div`/`script`/`body`/unknown targets keep flagging.

use oxc_ast::ast::{Expression, UnaryOperator};
use oxc_semantic::Semantic;

/// True when `expr` is a `TemplateLiteral` whose every interpolation is provably
/// numeric (see [`is_provably_numeric`]). A template with no interpolations is
/// a static string, handled by the per-rule static-string exemption, so this
/// returns `false` for it (`expressions.is_empty()` short-circuits via `all`).
#[must_use]
pub fn is_numeric_only_template(expr: &Expression) -> bool {
    let Expression::TemplateLiteral(tpl) = expr else {
        return false;
    };
    !tpl.expressions.is_empty() && tpl.expressions.iter().all(is_provably_numeric)
}

/// True when `expr` is structurally numeric, so it cannot inject HTML.
///
/// Covers numeric literals, unary `-`/`+`/`~`, the always-numeric arithmetic
/// operators (`- * / % **` and bitwise), `.length`, and the
/// `Number`/`parseInt`/`parseFloat`/`Math.*` builtins. Binary `+` is only
/// numeric when both operands are provably numeric, since `a + b` is string
/// concatenation when either side is a string. Builtins and `.length` are
/// matched by name against the global environment — a shadowing local binding
/// (`const Number = …`) is not resolved; everything unrecognized stays flagged.
fn is_provably_numeric(expr: &Expression) -> bool {
    match expr {
        Expression::NumericLiteral(_) => true,
        Expression::ParenthesizedExpression(p) => is_provably_numeric(&p.expression),
        Expression::UnaryExpression(u) => match u.operator {
            UnaryOperator::UnaryNegation
            | UnaryOperator::UnaryPlus
            | UnaryOperator::BitwiseNot => is_provably_numeric(&u.argument),
            _ => false,
        },
        Expression::BinaryExpression(bin) => {
            use oxc_ast::ast::BinaryOperator as Op;
            match bin.operator {
                Op::Subtraction
                | Op::Multiplication
                | Op::Division
                | Op::Remainder
                | Op::Exponential
                | Op::BitwiseOR
                | Op::BitwiseAnd
                | Op::BitwiseXOR
                | Op::ShiftLeft
                | Op::ShiftRight
                | Op::ShiftRightZeroFill => true,
                // `a + b` is string concatenation unless both sides are numbers.
                Op::Addition => {
                    is_provably_numeric(&bin.left) && is_provably_numeric(&bin.right)
                }
                _ => false,
            }
        }
        Expression::StaticMemberExpression(member) => member.property.name.as_str() == "length",
        Expression::CallExpression(call) => callee_is_numeric_builtin(&call.callee),
        _ => false,
    }
}

/// True when `callee` is one of the number-producing builtins: `Number(...)`,
/// `parseInt(...)`, `parseFloat(...)`, or any `Math.*(...)` method.
fn callee_is_numeric_builtin(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => {
            matches!(id.name.as_str(), "Number" | "parseInt" | "parseFloat")
        }
        Expression::StaticMemberExpression(member) => {
            matches!(&member.object, Expression::Identifier(obj) if obj.name.as_str() == "Math")
        }
        _ => false,
    }
}

/// True when the object an `innerHTML`/`outerHTML` assignment writes to is
/// provably a `<template>` element, so the write is an inert HTML parse rather
/// than an XSS sink.
///
/// `member_object` is the receiver of the assignment target
/// (`X` in `X.innerHTML = …`). It is a `<template>` only when:
/// - it is `(expr as HTMLTemplateElement)` — an explicit cast at the call site,
///   or
/// - it is an identifier whose binding is declared with an `HTMLTemplateElement`
///   type annotation, or initialised from a value provably yielding a
///   `<template>` (`document.createElement("template")` /
///   `document.createElementNS(<any>, "template")`).
///
/// A receiver's *name* is never evidence. Any other element type (`div`,
/// `script`, `body`, a custom element, an untyped parameter, a member chain)
/// returns `false`, so every non-template `innerHTML` write keeps flagging.
#[must_use]
pub fn lhs_object_is_template_element(member_object: &Expression, semantic: &Semantic) -> bool {
    match member_object {
        Expression::TSAsExpression(as_expr) => {
            type_is_html_template_element(&as_expr.type_annotation)
                || lhs_object_is_template_element(&as_expr.expression, semantic)
        }
        Expression::ParenthesizedExpression(paren) => {
            lhs_object_is_template_element(&paren.expression, semantic)
        }
        Expression::TSNonNullExpression(nn) => {
            lhs_object_is_template_element(&nn.expression, semantic)
        }
        Expression::Identifier(ident) => binding_is_template_element(ident, semantic),
        _ => false,
    }
}

/// Whether a type annotation is the DOM `HTMLTemplateElement` interface.
fn type_is_html_template_element(ty: &oxc_ast::ast::TSType) -> bool {
    use oxc_ast::ast::{TSType, TSTypeName};
    let TSType::TSTypeReference(tref) = ty else {
        return false;
    };
    matches!(
        &tref.type_name,
        TSTypeName::IdentifierReference(id) if id.name.as_str() == "HTMLTemplateElement"
    )
}

/// Resolve an identifier reference to its declaration and decide whether that
/// declaration proves the binding holds a `<template>` element — a declarator
/// with an `HTMLTemplateElement` type annotation or initialised from a
/// template-producing expression, or a parameter typed `HTMLTemplateElement`.
///
/// The initializer-based proof is trusted only for a `const` binding: a `let`/
/// `var` initialised from a `<template>` can be reassigned to a live-DOM element
/// (`let t = document.createElement("template"); t = document.createElement("div")`),
/// which `symbol_declaration` (declaration node only) cannot see — so the
/// security exemption fails closed there. A type annotation is value-invariant,
/// so it holds regardless of the declaration kind.
fn binding_is_template_element(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &Semantic,
) -> bool {
    use oxc_ast::AstKind;
    use oxc_ast::ast::VariableDeclarationKind;
    let scoping = semantic.scoping();
    let Some(symbol_id) = ident
        .reference_id
        .get()
        .and_then(|ref_id| scoping.get_reference(ref_id).symbol_id())
    else {
        return false;
    };
    let nodes = semantic.nodes();
    let decl_id = scoping.symbol_declaration(symbol_id);
    match nodes.kind(decl_id) {
        AstKind::VariableDeclarator(decl) => {
            if let Some(type_ann) = &decl.type_annotation
                && type_is_html_template_element(&type_ann.type_annotation)
            {
                return true;
            }
            decl.kind == VariableDeclarationKind::Const
                && decl.init.as_ref().is_some_and(initializer_is_template_element)
        }
        AstKind::FormalParameter(param) => param
            .type_annotation
            .as_ref()
            .is_some_and(|ann| type_is_html_template_element(&ann.type_annotation)),
        _ => false,
    }
}

/// Whether an initializer expression provably yields a `<template>` element:
/// `document.createElement("template")` or
/// `document.createElementNS(<any>, "template")`, possibly through a paren,
/// `as`/`!` wrapper, or the right-hand side of a logical assignment
/// (`a ||= document.createElement("template")` evaluates to that element).
fn initializer_is_template_element(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => call_creates_template_element(call),
        Expression::ParenthesizedExpression(paren) => {
            initializer_is_template_element(&paren.expression)
        }
        Expression::TSAsExpression(as_expr) => {
            type_is_html_template_element(&as_expr.type_annotation)
                || initializer_is_template_element(&as_expr.expression)
        }
        Expression::TSNonNullExpression(nn) => initializer_is_template_element(&nn.expression),
        Expression::AssignmentExpression(assign) => {
            initializer_is_template_element(&assign.right)
        }
        _ => false,
    }
}

/// Whether a call is `document.createElement("template")` or
/// `document.createElementNS(<ns>, "template")` — the two DOM APIs that mint a
/// `<template>` element. The tag-name argument must be the string literal
/// `"template"`; `createElement` takes it first, `createElementNS` second.
fn call_creates_template_element(call: &oxc_ast::ast::CallExpression) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let tag_arg_index = match member.property.name.as_str() {
        "createElement" => 0,
        "createElementNS" => 1,
        _ => return false,
    };
    call.arguments
        .get(tag_arg_index)
        .and_then(|arg| arg.as_expression())
        .is_some_and(|arg| {
            matches!(arg, Expression::StringLiteral(lit) if lit.value.as_str() == "template")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::SourceType;

    /// Parse `src` and return whether its single expression-statement RHS is a
    /// numeric-only template. `src` must be `x = <expr>;`.
    fn numeric_only(src: &str) -> bool {
        let allocator = Allocator::default();
        let ret = Parser::new(&allocator, src, SourceType::ts()).parse();
        let stmt = ret.program.body.first().expect("one statement");
        let oxc_ast::ast::Statement::ExpressionStatement(es) = stmt else {
            panic!("expected expression statement");
        };
        let Expression::AssignmentExpression(assign) = &es.expression else {
            panic!("expected assignment");
        };
        is_numeric_only_template(&assign.right)
    }

    #[test]
    fn numeric_literals_only() {
        assert!(numeric_only("x = `left: ${1}px; top: ${2}px`;"));
    }

    #[test]
    fn arithmetic_interpolations() {
        assert!(numeric_only("x = `w: ${10 * 2}px; h: ${a - b}px`;"));
    }

    // `+` is numeric when both operands are provably numeric.
    #[test]
    fn addition_of_two_numbers() {
        assert!(numeric_only("x = `n: ${1 + 2}`;"));
    }

    #[test]
    fn number_builtin() {
        assert!(numeric_only("x = `v: ${Number(s)}; m: ${Math.round(z)}`;"));
    }

    #[test]
    fn length_member() {
        assert!(numeric_only("x = `count: ${items.length}`;"));
    }

    #[test]
    fn bare_identifier_is_not_numeric() {
        // Without type info a bare identifier could be a string.
        assert!(!numeric_only("x = `<b>${userInput}</b>`;"));
    }

    #[test]
    fn string_concat_addition_is_not_numeric() {
        assert!(!numeric_only("x = `<b>${a + name}</b>`;"));
    }

    #[test]
    fn mixed_numeric_and_string_is_not_numeric() {
        assert!(!numeric_only("x = `${1}${userInput}`;"));
    }

    #[test]
    fn static_template_is_not_numeric_only() {
        // No interpolation: handled by the static-string exemption, not this one.
        assert!(!numeric_only("x = `<div></div>`;"));
    }

    #[test]
    fn non_template_rhs_is_not_numeric_only() {
        assert!(!numeric_only("x = userInput;"));
    }
}
