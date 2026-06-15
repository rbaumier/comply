//! OxcCheck backend for rn-no-literal-colors.
//!
//! Flags object properties whose name contains "color" (case-insensitive) and
//! whose value is a string literal — when they appear inside a React Native
//! style context: a JSX attribute whose name contains "style", or a
//! `StyleSheet.create(...)` call where `StyleSheet` is either an unresolved
//! global or imported from `react-native` / `react-native-web`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, JSXAttributeName, PropertyKey};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

const REACT_NATIVE_PACKAGES: &[&str] = &["react-native", "react-native-web"];

/// True when `name` contains "color" (case-insensitive), e.g. `color`,
/// `backgroundColor`, `borderBottomColor`, `fontColor`.
fn is_color_property(name: &str) -> bool {
    name.to_ascii_lowercase().contains("color")
}

/// True when `name` contains "style" (case-insensitive), e.g. `style`,
/// `contentContainerStyle`.
fn is_style_name(name: &str) -> bool {
    name.to_ascii_lowercase().contains("style")
}

/// A color literal is a string literal, or a ternary with a string-literal
/// consequent or alternate.
fn has_color_literal_value(value: &Expression) -> bool {
    match value {
        Expression::StringLiteral(_) => true,
        Expression::ConditionalExpression(cond) => {
            matches!(cond.consequent, Expression::StringLiteral(_))
                || matches!(cond.alternate, Expression::StringLiteral(_))
        }
        _ => false,
    }
}

/// True when `call` is `StyleSheet.create(...)` referring to React Native's
/// `StyleSheet`. The object must be a plain `StyleSheet` reference that is
/// either unresolved (a global) or bound to an import from a React Native
/// package — a local declaration or an import from another package is not
/// React Native's `StyleSheet`.
fn is_react_native_stylesheet_create<'a>(
    call: &oxc_ast::ast::CallExpression<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    if member.property.name.as_str() != "create" {
        return false;
    }
    let Expression::Identifier(object) = &member.object else {
        return false;
    };
    if object.name.as_str() != "StyleSheet" {
        return false;
    }

    let Some(ref_id) = object.reference_id.get() else {
        // Unresolved reference (no binding) — a global `StyleSheet`.
        return true;
    };
    let scoping = semantic.scoping();
    let Some(symbol_id) = scoping.get_reference(ref_id).symbol_id() else {
        return true;
    };
    // Resolved binding: only an import from a React Native package qualifies.
    let decl_node_id = scoping.symbol_declaration(symbol_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
    {
        if let AstKind::ImportDeclaration(import) = kind {
            return REACT_NATIVE_PACKAGES.contains(&import.source.value.as_str());
        }
    }
    false
}

/// True when `node` is a descendant of a React Native style context: a JSX
/// attribute whose name contains "style", or a qualifying `StyleSheet.create`
/// call.
fn in_style_context<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::JSXAttribute(attr) => {
                if let JSXAttributeName::Identifier(name) = &attr.name
                    && is_style_name(name.name.as_str())
                {
                    return true;
                }
            }
            AstKind::CallExpression(call) => {
                if is_react_native_stylesheet_create(call, semantic) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };
        let key_name = match &prop.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => return,
        };
        if !is_color_property(key_name) {
            return;
        }
        if !has_color_literal_value(&prop.value) {
            return;
        }
        if !in_style_context(node, semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, prop.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Color literal in a React Native style — move it to a named constant or theme variable.".into(),
            severity: Severity::Warning,
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
