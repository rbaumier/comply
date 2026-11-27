//! Tailwind class strings a TS/JS file builds, split into classes.

use oxc_ast::AstKind;
use oxc_ast::ast::{
    CallExpression, Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue,
    JSXOpeningElement, ObjectPropertyKind, TemplateLiteral,
};
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

/// Callees whose string arguments are Tailwind class lists.
pub const CLASS_HELPERS: &[&str] = &[
    "cn",
    "clsx",
    "classnames",
    "cva",
    "tw",
    "twMerge",
    "twJoin",
    "clx",
];

/// One string literal, or one whole template, holding classes.
pub struct ClassString<'a> {
    pub span: Span,
    pub classes: Vec<Class<'a>>,
    /// Innermost `?:` arm or `&&`/`||` operand; `None` when unconditional.
    pub branch: Option<Span>,
    pub in_iteration_callback: bool,
}

/// One class: its variants (`hover`, `md`) and its utility, `!` dropped.
pub struct Class<'a> {
    pub raw: &'a str,
    pub variants: Vec<&'a str>,
    pub utility: &'a str,
}

/// Every class string reaching `className`/`class` or a [`CLASS_HELPERS`] call.
#[must_use]
pub fn class_strings<'a>(semantic: &'a Semantic<'a>) -> Vec<ClassString<'a>> {
    let mut strings = Vec::new();
    let mut nested = Vec::new();
    for node in semantic.nodes().iter() {
        if let AstKind::CallExpression(call) = node.kind()
            && nested.contains(&call.span)
        {
            continue;
        }
        let first = strings.len();
        collect_entry_point(node.kind(), semantic, &mut strings, &mut nested);
        if strings.len() > first && in_iteration_callback(node.id(), semantic) {
            for string in &mut strings[first..] {
                string.in_iteration_callback = true;
            }
        }
    }
    strings
}

/// The class strings of one element's own attributes, constants resolved.
#[must_use]
pub fn element_class_strings<'a>(
    opening: &'a JSXOpeningElement<'a>,
    semantic: &'a Semantic<'a>,
) -> Vec<ClassString<'a>> {
    let mut strings = Vec::new();
    let mut nested = Vec::new();
    for attr in opening
        .attributes
        .iter()
        .filter_map(JSXAttributeItem::as_attribute)
    {
        collect_entry_point(
            AstKind::JSXAttribute(attr),
            semantic,
            &mut strings,
            &mut nested,
        );
    }
    strings
}

fn collect_entry_point<'a>(
    kind: AstKind<'a>,
    semantic: &'a Semantic<'a>,
    strings: &mut Vec<ClassString<'a>>,
    nested: &mut Vec<Span>,
) {
    match kind {
        AstKind::JSXAttribute(attr) => {
            let JSXAttributeName::Identifier(name) = &attr.name else {
                return;
            };
            if !matches!(name.name.as_str(), "className" | "class") {
                return;
            }
            match &attr.value {
                Some(JSXAttributeValue::StringLiteral(lit)) => {
                    strings.push(class_string(lit.span, &[lit.value.as_str()], None));
                }
                Some(JSXAttributeValue::ExpressionContainer(container)) => {
                    if let Some(expr) = container.expression.as_expression() {
                        collect(expr, None, true, semantic, strings, nested);
                    }
                }
                _ => {}
            }
        }
        AstKind::CallExpression(call) if is_class_helper(call) => {
            for expr in call.arguments.iter().filter_map(|arg| arg.as_expression()) {
                collect(expr, None, true, semantic, strings, nested);
            }
        }
        _ => {}
    }
}

fn is_class_helper(call: &CallExpression) -> bool {
    let callee = match &call.callee {
        Expression::Identifier(id) => id.name.as_str(),
        Expression::StaticMemberExpression(member) => member.property.name.as_str(),
        _ => return false,
    };
    CLASS_HELPERS.contains(&callee)
}

/// Walks down to the literals. Constants resolve one level only.
fn collect<'a>(
    expr: &'a Expression<'a>,
    branch: Option<Span>,
    can_resolve: bool,
    semantic: &'a Semantic<'a>,
    strings: &mut Vec<ClassString<'a>>,
    nested: &mut Vec<Span>,
) {
    match expr.get_inner_expression() {
        Expression::StringLiteral(lit) => {
            strings.push(class_string(lit.span, &[lit.value.as_str()], branch));
        }
        Expression::TemplateLiteral(template) => {
            strings.push(template_string(template, branch));
            for inner in &template.expressions {
                collect(inner, branch, can_resolve, semantic, strings, nested);
            }
        }
        Expression::ConditionalExpression(cond) => {
            for arm in [&cond.consequent, &cond.alternate] {
                collect(
                    arm,
                    Some(arm.span()),
                    can_resolve,
                    semantic,
                    strings,
                    nested,
                );
            }
        }
        Expression::LogicalExpression(logical) => {
            for operand in [&logical.left, &logical.right] {
                collect(
                    operand,
                    Some(operand.span()),
                    can_resolve,
                    semantic,
                    strings,
                    nested,
                );
            }
        }
        Expression::ArrayExpression(array) => {
            for element in array.elements.iter().filter_map(|e| e.as_expression()) {
                collect(element, branch, can_resolve, semantic, strings, nested);
            }
        }
        // A resolved constant's helper call is its own entry point.
        Expression::CallExpression(call) if can_resolve && is_class_helper(call) => {
            nested.push(call.span);
            for arg in call.arguments.iter().filter_map(|arg| arg.as_expression()) {
                collect(arg, branch, can_resolve, semantic, strings, nested);
            }
        }
        Expression::ObjectExpression(object) => {
            for property in &object.properties {
                if let ObjectPropertyKind::ObjectProperty(property) = property {
                    collect(
                        &property.value,
                        branch,
                        can_resolve,
                        semantic,
                        strings,
                        nested,
                    );
                }
            }
        }
        Expression::Identifier(ident) if can_resolve => {
            if let Some(init) = constant_init(ident, semantic) {
                collect(init, branch, false, semantic, strings, nested);
            }
        }
        _ => {}
    }
}

fn constant_init<'a>(
    ident: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &'a Semantic<'a>,
) -> Option<&'a Expression<'a>> {
    let scoping = semantic.scoping();
    let symbol = scoping
        .get_reference(ident.reference_id.get()?)
        .symbol_id()?;
    match semantic.nodes().kind(scoping.symbol_declaration(symbol)) {
        AstKind::VariableDeclarator(declarator) if declarator.id.is_binding_identifier() => {
            declarator.init.as_ref()
        }
        _ => None,
    }
}

/// True inside a function passed to `.map` or `.flatMap`.
fn in_iteration_callback(id: NodeId, semantic: &Semantic) -> bool {
    let ancestors = semantic.nodes().ancestor_kinds(id);
    ancestors
        .clone()
        .zip(ancestors.skip(1))
        .any(|(child, parent)| {
            matches!(
                child,
                AstKind::ArrowFunctionExpression(_) | AstKind::Function(_)
            ) && matches!(parent, AstKind::CallExpression(call)
                if matches!(&call.callee, Expression::StaticMemberExpression(member)
                    if matches!(member.property.name.as_str(), "map" | "flatMap")))
        })
}

fn template_string<'a>(template: &'a TemplateLiteral<'a>, branch: Option<Span>) -> ClassString<'a> {
    let pieces: Vec<&'a str> = template
        .quasis
        .iter()
        .map(|q| q.value.raw.as_str())
        .collect();
    class_string(template.span, &pieces, branch)
}

fn class_string<'a>(span: Span, pieces: &[&'a str], branch: Option<Span>) -> ClassString<'a> {
    ClassString {
        span,
        classes: pieces
            .iter()
            .flat_map(|piece| piece.split_whitespace())
            .map(parse_class)
            .collect(),
        branch,
        in_iteration_callback: false,
    }
}

fn parse_class(token: &str) -> Class<'_> {
    let raw = token.trim_matches('!');
    let mut segments = Vec::new();
    let mut depth = 0_u32;
    let mut start = 0;
    for (i, c) in raw.char_indices() {
        match c {
            '[' | '(' => depth = depth.saturating_add(1),
            ']' | ')' => depth = depth.saturating_sub(1),
            ':' if depth == 0 => {
                segments.push(&raw[start..i]);
                start = i.saturating_add(1);
            }
            _ => {}
        }
    }
    Class {
        raw,
        variants: segments,
        utility: raw[start..].trim_matches('!'),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn marks_conditional_branch() {
        let src = r#"cn("a", ok && "b", ok ? "c" : "d");
            const x = <i className={ok ? cn("e") : "f"} />;
            cn(ok && cn("g"));"#;
        let flags = crate::oxc_helpers::with_oxc_parse(src, Path::new("t.tsx"), |semantic| {
            class_strings(semantic)
                .iter()
                .flat_map(|s| {
                    s.classes
                        .iter()
                        .map(|c| (c.utility.to_string(), s.branch.is_some()))
                })
                .collect::<Vec<_>>()
        });
        let expected = [
            ("a", false),
            ("b", true),
            ("c", true),
            ("d", true),
            ("e", true),
            ("f", true),
            ("g", true),
        ];
        assert_eq!(flags, expected.map(|(u, f)| (u.to_string(), f)).to_vec());
    }
}
