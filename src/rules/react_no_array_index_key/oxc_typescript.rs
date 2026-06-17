//! react-no-array-index-key oxc backend for TSX.
//!
//! Flags `.map((item, i) => <X key={i} />)` — array indices as React keys
//! break on reorder/filter/insert.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, ArrayExpressionElement, BindingPattern, Expression, JSXAttributeName,
    JSXAttributeValue, JSXExpression, ObjectPropertyKind, PropertyKey,
};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        // Must be a `key` prop.
        let JSXAttributeName::Identifier(name_ident) = &attr.name else {
            return;
        };
        if name_ident.name.as_str() != "key" {
            return;
        }
        // Value must be {identifier} — a simple variable reference.
        let Some(JSXAttributeValue::ExpressionContainer(container)) = &attr.value else {
            return;
        };
        let key_name = match &container.expression {
            JSXExpression::Identifier(ident) => ident.name.as_str(),
            _ => return,
        };

        // Walk ancestors to find an enclosing `.map(callback)` call
        // whose callback's second parameter matches key_name.
        if !inside_map_with_index(node, semantic, key_name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "react-no-array-index-key".into(),
            message: "`key={index}` breaks on reorder / filter / insert \u{2014} React \
                      associates the wrong DOM state with the wrong item. Use a \
                      stable id from the data."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn inside_map_with_index<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    key_name: &str,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        let AstKind::CallExpression(call) = ancestor.kind() else {
            continue;
        };
        // Callee must be `*.map`.
        let Expression::StaticMemberExpression(member) = call.callee.without_parentheses() else {
            continue;
        };
        if member.property.name.as_str() != "map" {
            continue;
        }
        // The receiver being mapped is a statically-constructed fixed array
        // (skeleton/placeholder): it has no backing data and never
        // reorders/filters/inserts, so the index is the only correct key.
        if receiver_is_static_array(member.object.without_parentheses()) {
            continue;
        }
        // First argument must be an arrow/function with a second param matching key_name.
        let Some(first_arg) = call.arguments.first() else {
            continue;
        };
        let Argument::ArrowFunctionExpression(arrow) = first_arg else {
            // Also try function expression.
            let Argument::FunctionExpression(func) = first_arg else {
                continue;
            };
            if second_param_matches(&func.params, key_name) {
                return true;
            }
            continue;
        };
        if second_param_matches(&arrow.params, key_name) {
            return true;
        }
    }
    false
}

/// `true` when `expr` is a statically-constructed fixed-length array literal
/// with no backing data — a skeleton/placeholder list that can never reorder,
/// filter or insert. Recognizes exactly:
/// - `Array(N).fill(...)` / `new Array(N).fill(...)`
/// - `Array.from({ length: N })` (NOT `Array.from(data)` — that maps real data)
/// - `[...Array(N)]` / `[...new Array(N)]`
///
/// Fail-closed: anything else (plain identifiers, dynamic spreads,
/// `Array.from(data)`, bare element literals) returns `false` and stays flagged.
fn receiver_is_static_array(expr: &Expression) -> bool {
    match expr {
        // `Array(N).fill(...)` / `new Array(N).fill(...)` and `Array.from({ length })`.
        Expression::CallExpression(call) => {
            let Expression::StaticMemberExpression(member) =
                call.callee.without_parentheses()
            else {
                return false;
            };
            match member.property.name.as_str() {
                "fill" => is_array_construction(member.object.without_parentheses()),
                "from" => {
                    is_array_identifier(member.object.without_parentheses())
                        && first_arg_is_length_object(&call.arguments)
                }
                _ => false,
            }
        }
        // `[...Array(N)]` — a single spread of an `Array(N)` construction.
        Expression::ArrayExpression(array) => match array.elements.as_slice() {
            [ArrayExpressionElement::SpreadElement(spread)] => {
                is_array_construction(spread.argument.without_parentheses())
            }
            _ => false,
        },
        _ => false,
    }
}

/// `true` for `Array(...)` (call) or `new Array(...)` (new) with an `Array` callee.
fn is_array_construction(expr: &Expression) -> bool {
    match expr {
        Expression::CallExpression(call) => {
            is_array_identifier(call.callee.without_parentheses())
        }
        Expression::NewExpression(new_expr) => {
            is_array_identifier(new_expr.callee.without_parentheses())
        }
        _ => false,
    }
}

/// `true` when `expr` is the bare `Array` identifier.
fn is_array_identifier(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(ident) if ident.name.as_str() == "Array")
}

/// `true` when the first argument is an object literal carrying a `length`
/// property — the `Array.from({ length: N })` placeholder form. A non-object
/// argument (`Array.from(data)`) maps real data and must stay flagged.
fn first_arg_is_length_object(arguments: &[Argument]) -> bool {
    let Some(Argument::ObjectExpression(obj)) = arguments.first() else {
        return false;
    };
    obj.properties.iter().any(|prop| {
        matches!(
            prop,
            ObjectPropertyKind::ObjectProperty(p)
                if matches!(
                    &p.key,
                    PropertyKey::StaticIdentifier(k) if k.name.as_str() == "length"
                )
        )
    })
}

fn second_param_matches(params: &oxc_ast::ast::FormalParameters, key_name: &str) -> bool {
    let items = &params.items;
    if items.len() < 2 {
        return false;
    }
    let second = &items[1];
    let BindingPattern::BindingIdentifier(ident) = &second.pattern else {
        return false;
    };
    ident.name.as_str() == key_name
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_map_with_index_key() {
        let source = "const x = items.map((item, i) => <div key={i}>{item}</div>);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_stable_id_key() {
        let source = "const x = items.map(item => <div key={item.id}>{item.name}</div>);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_index_key_on_array_fill_skeleton() {
        let source = "const x = Array(12).fill(0).map((_, i) => <div key={i} />);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_index_key_on_new_array_fill_skeleton() {
        let source = "const x = new Array(6).fill(0).map((_, i) => <div key={i} />);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_index_key_on_array_from_length() {
        let source = "const x = Array.from({ length: 6 }).map((_, i) => <div key={i} />);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_index_key_on_spread_array() {
        let source = "const x = [...Array(8)].map((_, i) => <div key={i} />);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_array_from_real_data() {
        // `Array.from(data)` maps real data and CAN reorder — must still flag.
        let source = "const x = Array.from(realData).map((item, i) => <div key={i} />);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_spread_of_dynamic_source() {
        // `[...someData]` spreads a dynamic source — real data, must still flag.
        let source = "const x = [...someData].map((item, i) => <div key={i} />);";
        assert_eq!(run_on(source).len(), 1);
    }
}
