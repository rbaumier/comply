//! OxcCheck backend for no-solid-destructured-props.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingIdentifier, BindingPattern, ObjectPattern};
use oxc_semantic::ReferenceFlags;
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ArrowFunctionExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Solid-only: destructuring props is idiomatic in React. Gate before any
        // AST work so React/Vue/Preact files never fire.
        if !crate::oxc_helpers::is_solid_file(ctx.source, ctx.project, ctx.path) {
            return;
        }

        let AstKind::ArrowFunctionExpression(arrow) = node.kind() else {
            return;
        };

        // A Solid component is the arrow assigned to a PascalCase variable.
        let nodes = semantic.nodes();
        let Some(parent) = nodes.ancestors(node.id()).next() else {
            return;
        };
        let AstKind::VariableDeclarator(decl) = parent.kind() else {
            return;
        };
        let BindingPattern::BindingIdentifier(id) = &decl.id else {
            return;
        };
        if !is_pascal_case(id.name.as_str()) {
            return;
        }

        // A Solid component takes exactly one parameter.
        if arrow.params.rest.is_some() || arrow.params.items.len() != 1 {
            return;
        }
        let Some(param) = arrow.params.items.first() else {
            return;
        };
        let Some(object_pattern) = as_object_pattern(&param.pattern) else {
            return;
        };

        // Empty destructuring `{}` is a violation on its own.
        if object_pattern.properties.is_empty() && object_pattern.rest.is_none() {
            diagnostics.push(make_diagnostic(ctx, object_pattern.span));
            return;
        }

        // Otherwise flag each destructured binding read inside a JSX attribute
        // value (`<div a={foo} />`). One diagnostic per binding, at its first
        // such read.
        let mut bindings = Vec::new();
        collect_bindings(object_pattern, &mut bindings);
        for binding in bindings {
            if let Some(span) = first_jsx_prop_read(binding, semantic) {
                diagnostics.push(make_diagnostic(ctx, span));
            }
        }
    }
}

/// Solid components are detected by a PascalCase variable name — the first
/// character is an uppercase ASCII letter.
fn is_pascal_case(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// The object pattern of a parameter, transparent to a default value
/// (`{ ... } = {}` is an `AssignmentPattern` wrapping the object pattern).
fn as_object_pattern<'p>(pattern: &'p BindingPattern) -> Option<&'p ObjectPattern<'p>> {
    match pattern {
        BindingPattern::ObjectPattern(obj) => Some(obj),
        BindingPattern::AssignmentPattern(assign) => as_object_pattern(&assign.left),
        _ => None,
    }
}

/// Flatten every leaf binding identifier of an object pattern, descending into
/// nested object/array patterns, default values, and rest elements — mirroring
/// the set of names Solid would freeze by destructuring.
fn collect_bindings<'a>(pattern: &'a ObjectPattern<'a>, out: &mut Vec<&'a BindingIdentifier<'a>>) {
    for prop in &pattern.properties {
        collect_from_binding_pattern(&prop.value, out);
    }
    if let Some(rest) = &pattern.rest {
        collect_from_binding_pattern(&rest.argument, out);
    }
}

fn collect_from_binding_pattern<'a>(
    pattern: &'a BindingPattern<'a>,
    out: &mut Vec<&'a BindingIdentifier<'a>>,
) {
    match pattern {
        BindingPattern::BindingIdentifier(id) => out.push(id),
        BindingPattern::AssignmentPattern(assign) => {
            collect_from_binding_pattern(&assign.left, out);
        }
        BindingPattern::ObjectPattern(obj) => collect_bindings(obj, out),
        BindingPattern::ArrayPattern(arr) => {
            for element in arr.elements.iter().flatten() {
                collect_from_binding_pattern(element, out);
            }
            if let Some(rest) = &arr.rest {
                collect_from_binding_pattern(&rest.argument, out);
            }
        }
    }
}

/// The span of the first read of `binding` that sits inside a JSX expression
/// attribute value (`<div a={binding} />` or `<div a={binding.x} />`), or
/// `None` if the binding is never used that way.
fn first_jsx_prop_read(
    binding: &BindingIdentifier,
    semantic: &oxc_semantic::Semantic,
) -> Option<Span> {
    let symbol_id = binding.symbol_id.get()?;
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    for reference in scoping.get_resolved_references(symbol_id) {
        if !reference.flags().contains(ReferenceFlags::Read) {
            continue;
        }
        let ref_id = reference.node_id();
        if is_in_jsx_attribute_value(ref_id, nodes) {
            return Some(nodes.kind(ref_id).span());
        }
    }
    None
}

/// True when `node_id`'s ancestors include a `JSXExpressionContainer` whose
/// parent is a `JSXAttribute` — i.e. the node is part of an attribute value
/// expression, not a JSX child or some other expression.
fn is_in_jsx_attribute_value(node_id: oxc_semantic::NodeId, nodes: &oxc_semantic::AstNodes) -> bool {
    for ancestor in nodes.ancestors(node_id) {
        if let AstKind::JSXExpressionContainer(_) = ancestor.kind() {
            let parent_id = nodes.parent_id(ancestor.id());
            return matches!(nodes.kind(parent_id), AstKind::JSXAttribute(_));
        }
    }
    false
}

fn make_diagnostic(ctx: &CheckCtx, span: Span) -> Diagnostic {
    let (line, column) = byte_offset_to_line_col(ctx.source, span.start as usize);
    Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "Solid component props must not be destructured — reading \
                  `props.foo` is what preserves reactivity."
            .into(),
        severity: Severity::Warning,
        span: None,
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Solid context: an import from `solid-js` flips on the framework gate.
    const SOLID: &str = "import { createSignal } from \"solid-js\";\n";

    fn run(src: &str) -> Vec<Diagnostic> {
        let full = format!("{SOLID}{src}");
        crate::rules::test_helpers::run_rule(&Check, &full, "t.tsx")
    }

    fn count(src: &str) -> usize {
        run(src).len()
    }

    // ---- Biome `valid.tsx` fixtures: no diagnostics ----

    #[test]
    fn valid_props_param_no_destructure() {
        assert_eq!(count("let Component = (props) => <div />;"), 0);
    }

    #[test]
    fn valid_props_member_access() {
        assert_eq!(count("let Component = (props) => <div a={props.a} />;"), 0);
    }

    #[test]
    fn valid_split_props() {
        let src = "let Component = (props) => {\n\
                   const [local, rest] = splitProps(props, [\"a\"]);\n\
                   return <div a={local.a} b={rest.b} />;\n\
                   };";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn valid_destructure_local_call_result() {
        let src = "let Component = (props) => {\n\
                   const { a } = someFunction();\n\
                   return <div a={a} />;\n\
                   };";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn valid_not_a_component_multiple_params() {
        assert_eq!(count("let NotAComponent = ({ a }, more, params) => <div a={a} />;"), 0);
    }

    #[test]
    fn valid_inner_arrow_not_pascal() {
        let src = "let Component = (props) => {\n\
                   let inner = ({ a, ...rest }) => a;\n\
                   let a = inner({ a: 5 });\n\
                   return <div a={a} />;\n\
                   };";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn valid_destructure_props_in_body_not_params() {
        // Destructuring `props` in the body (not the parameter) is out of scope —
        // Biome leaves that to the solid/reactivity rule.
        let src = "let Component = (props) => {\n\
                   let { a } = props;\n\
                   return <div a={a} />;\n\
                   };";
        assert_eq!(count(src), 0);
    }

    #[test]
    fn valid_props_typed_param() {
        assert_eq!(count("let Component = (props: Props) => <div />;"), 0);
    }

    // ---- Biome `invalid.tsx` fixtures: one diagnostic per flagged binding ----

    #[test]
    fn invalid_empty_destructure() {
        assert_eq!(count("let Component = ({}) => <div />;"), 1);
    }

    #[test]
    fn invalid_single_shorthand() {
        assert_eq!(count("let Component = ({ a }) => <div a={a} />;"), 1);
    }

    #[test]
    fn invalid_renamed() {
        assert_eq!(count("let Component = ({ a: A }) => <div a={A} />;"), 1);
    }

    #[test]
    fn invalid_computed_key() {
        assert_eq!(count("let Component = ({ [\"a\" + \"\"]: a }) => <div a={a} />;"), 1);
    }

    #[test]
    fn invalid_computed_key_plus_shorthand() {
        assert_eq!(
            count("let Component = ({ [\"a\" + \"\"]: a, b }) => <div a={a} b={b} />;"),
            2
        );
    }

    #[test]
    fn invalid_default_value() {
        assert_eq!(count("let Component = ({ a = 5 }) => <div a={a} />;"), 1);
    }

    #[test]
    fn invalid_renamed_with_default() {
        assert_eq!(count("let Component = ({ a: A = 5 }) => <div a={A} />;"), 1);
    }

    #[test]
    fn invalid_three_bindings_mixed() {
        assert_eq!(
            count("let Component = ({ [\"a\" + \"\"]: a = 5, b = 10, c }) => <div a={a} b={b} c={c} />;"),
            3
        );
    }

    #[test]
    fn invalid_block_body() {
        let src = "let Component = ({ a = 5 }) => {\n\
                   return <div a={a} />;\n\
                   };";
        assert_eq!(count(src), 1);
    }

    #[test]
    fn invalid_block_body_with_statements() {
        let src = "let Component = ({ a = 5 }) => {\n\
                   various();\n\
                   statements();\n\
                   return <div a={a} />;\n\
                   };";
        assert_eq!(count(src), 1);
    }

    #[test]
    fn invalid_rest_only_member_access() {
        assert_eq!(count("let Component = ({ ...rest }) => <div a={rest.a} />;"), 1);
    }

    #[test]
    fn invalid_shorthand_plus_unused_rest() {
        // `rest` is destructured but never read in a JSX attr → only `a` fires.
        assert_eq!(count("let Component = ({ a, ...rest }) => <div a={a} />;"), 1);
    }

    #[test]
    fn invalid_shorthand_plus_rest_member_access() {
        assert_eq!(
            count("let Component = ({ a, ...rest }) => <div a={a} b={rest.b} />;"),
            2
        );
    }

    #[test]
    fn invalid_renamed_plus_rest() {
        assert_eq!(count("let Component = ({ a: A, ...rest }) => <div a={A} />;"), 1);
    }

    #[test]
    fn invalid_typed_two_props() {
        assert_eq!(
            count("let Component = ({ prop1, prop2 }: Props) => <div p1={prop1} p2={prop2} />;"),
            2
        );
    }

    // ---- Framework gate guards (both directions) ----

    #[test]
    fn react_file_does_not_fire() {
        // Idiomatic React: destructured props must NOT fire (no Solid signal).
        let src = "import { useState } from \"react\";\n\
                   let Component = ({ a }) => <div a={a} />;";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert!(d.is_empty(), "React file should not fire: {d:?}");
    }

    #[test]
    fn plain_file_without_solid_signal_does_not_fire() {
        // No framework import at all → no Solid signal → no fire.
        let src = "let Component = ({ a }) => <div a={a} />;";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert!(d.is_empty(), "non-Solid file should not fire: {d:?}");
    }

    #[test]
    fn solid_file_with_destructured_props_fires() {
        // Positive: the exact same code in a Solid file MUST fire.
        let src = "import { createSignal } from \"solid-js\";\n\
                   let Component = ({ a }) => <div a={a} />;";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert_eq!(d.len(), 1, "Solid file should fire: {d:?}");
    }

    #[test]
    fn solid_jsx_import_source_pragma_fires() {
        let src = "/** @jsxImportSource solid-js */\n\
                   let Component = ({ a }) => <div a={a} />;";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert_eq!(d.len(), 1, "solid-js pragma should fire: {d:?}");
    }

    #[test]
    fn issue_5449_jsx_attr_arrow_param_destructure() {
        // A destructured param of an inline arrow passed as a JSX attribute is
        // not a component (the arrow isn't assigned to a PascalCase variable).
        let src = "export const ModelSelector = () => {\n\
                   return (\n\
                   <Select onChange={({ value }) => { console.log(value) }} />\n\
                   )\n\
                   };";
        assert_eq!(count(src), 0);
    }
}
