//! react-no-array-index-key oxc backend for TSX.
//!
//! Flags `.map((item, i) => <X key={i} />)` — array indices as React keys
//! break on reorder/filter/insert.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BindingPattern, Expression, JSXAttributeName,
    JSXAttributeValue, JSXExpression,
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
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
}
