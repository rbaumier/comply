//! OxcCheck backend for use-solid-for-component.

use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{CallExpression, ChainElement, Expression, JSXExpression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXExpressionContainer]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Solid-only: `array.map()` rendering JSX is idiomatic in React. Gate
        // before any AST work so React/Vue/Preact files never fire.
        if !crate::oxc_helpers::is_solid_file(ctx.source, ctx.project, ctx.path) {
            return;
        }

        let AstKind::JSXExpressionContainer(container) = node.kind() else {
            return;
        };

        // Only a container that is a JSX *child* (`<ol>{...}</ol>` or `<>{...}</>`)
        // counts. A container in an attribute value (`<div a={items.map(...)} />`)
        // is left alone — the parent is a `JSXAttribute`, not an element/fragment.
        if !is_jsx_child_container(node, semantic) {
            return;
        }

        // The container's expression must be a `.map(callback)` call with exactly
        // one argument. Optional chaining (`items?.map(...)`) wraps the call in a
        // `ChainExpression`.
        let Some(call) = map_call(&container.expression) else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`array.map()` in JSX recreates every DOM element on update \u{2014} \
                      use Solid's `<For>` component to render this list."
                .into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

/// True when the container is rendered as a child of a JSX element or fragment,
/// as opposed to sitting inside a JSX attribute value.
fn is_jsx_child_container(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    matches!(
        semantic.nodes().parent_node(node.id()).kind(),
        AstKind::JSXElement(_) | AstKind::JSXFragment(_)
    )
}

/// Returns the `.map(callback)` call when `expression` is a member call to `map`
/// with exactly one argument, transparently unwrapping optional chaining
/// (`items?.map(...)`). Returns `None` for anything else.
fn map_call<'a, 'b>(expression: &'b JSXExpression<'a>) -> Option<&'b CallExpression<'a>> {
    let call = match expression {
        JSXExpression::CallExpression(call) => call.as_ref(),
        JSXExpression::ChainExpression(chain) => match &chain.expression {
            ChainElement::CallExpression(call) => call.as_ref(),
            _ => return None,
        },
        _ => return None,
    };

    if call.arguments.len() != 1 {
        return None;
    }
    (callee_member_name(&call.callee)? == "map").then_some(call)
}

/// The accessed property name of a member-expression callee, covering both
/// `obj.map` and `obj["map"]`. `None` when the callee is not a member access.
fn callee_member_name<'a>(callee: &'a Expression) -> Option<&'a str> {
    match callee {
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        Expression::ComputedMemberExpression(member) => match &member.expression {
            Expression::StringLiteral(lit) => Some(lit.value.as_str()),
            _ => None,
        },
        _ => None,
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
    const SOLID: &str = "import { For } from \"solid-js\";\n";

    fn run(src: &str) -> Vec<Diagnostic> {
        let full = format!("{SOLID}{src}");
        crate::rules::test_helpers::run_rule(&Check, &full, "t.tsx")
    }

    fn count(src: &str) -> usize {
        run(src).len()
    }

    // ---- Biome `invalid.tsx` fixtures: one diagnostic each ----

    #[test]
    fn invalid_arrow_jsx_body_in_element() {
        assert_eq!(
            count("let Component = (props) => <ol>{props.data.map(d => <li>{d.text}</li>)}</ol>;"),
            1
        );
    }

    #[test]
    fn invalid_arrow_jsx_body_in_fragment() {
        assert_eq!(
            count("let Component = (props) => <>{props.data.map(d => <li>{d.text}</li>)}</>;"),
            1
        );
    }

    #[test]
    fn invalid_arrow_jsx_with_key() {
        assert_eq!(
            count("let Component = (props) => <ol>{props.data.map(d => <li key={d.id}>{d.text}</li>)}</ol>;"),
            1
        );
    }

    #[test]
    fn invalid_block_body_return() {
        let src = "function Component(props) {\n\
                   return <ol>{props.data.map(d => <li>{d.text}</li>)}</ol>;\n\
                   }";
        assert_eq!(count(src), 1);
    }

    #[test]
    fn invalid_optional_chaining_map() {
        let src = "function Component(props) {\n\
                   return <ol>{props.data?.map(d => <li>{d.text}</li>)}</ol>;\n\
                   }";
        assert_eq!(count(src), 1);
    }

    #[test]
    fn invalid_no_callback_param() {
        assert_eq!(
            count("let Component = (props) => <ol>{props.data.map(() => <li />)}</ol>;"),
            1
        );
    }

    #[test]
    fn invalid_rest_callback_param() {
        assert_eq!(
            count("let Component = (props) => <ol>{props.data.map((...args) => <li>{args[0].text}</li>)}</ol>;"),
            1
        );
    }

    // ---- Biome `valid.tsx` fixtures: no diagnostics ----

    #[test]
    fn valid_for_component() {
        assert_eq!(
            count("let Component = (props) => <ol><For each={props.data}>{d => <li>{d.text}</li>}</For></ol>;"),
            0
        );
    }

    #[test]
    fn valid_map_outside_jsx() {
        assert_eq!(count("let abc = x.map(y => y + z);"), 0);
    }

    #[test]
    fn valid_map_in_component_body_not_jsx() {
        let src = "let Component = (props) => {\n\
                   let abc = x.map(y => y + z);\n\
                   return <div>Hello, world!</div>;\n\
                   }";
        assert_eq!(count(src), 0);
    }

    // ---- Extra coverage: arity, attribute position, other methods ----

    #[test]
    fn valid_map_in_jsx_attribute_value() {
        // A container in an attribute value is not a JSX child — Biome only
        // flags `.map()` rendered as a child.
        assert_eq!(
            count("let Component = (props) => <ol data-x={props.data.map(d => d.id)} />;"),
            0
        );
    }

    #[test]
    fn valid_map_two_args_not_flagged() {
        // Biome requires exactly one argument (the callback).
        assert_eq!(
            count("let Component = (props) => <ol>{props.data.map(d => <li />, this)}</ol>;"),
            0
        );
    }

    #[test]
    fn valid_other_array_method() {
        assert_eq!(
            count("let Component = (props) => <ol>{props.data.filter(d => d.ok)}</ol>;"),
            0
        );
    }

    // ---- Framework gate guards (both directions) ----

    #[test]
    fn react_file_does_not_fire() {
        // Idiomatic React: `array.map()` in JSX must NOT fire (no Solid signal).
        let src = "import { useState } from \"react\";\n\
                   let Component = (props) => <ol>{props.data.map(d => <li>{d.text}</li>)}</ol>;";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert!(d.is_empty(), "React file should not fire: {d:?}");
    }

    #[test]
    fn plain_file_without_solid_signal_does_not_fire() {
        // No framework import at all → no Solid signal → no fire.
        let src = "let Component = (props) => <ol>{props.data.map(d => <li>{d.text}</li>)}</ol>;";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert!(d.is_empty(), "non-Solid file should not fire: {d:?}");
    }

    #[test]
    fn solid_jsx_import_source_pragma_fires() {
        let src = "/** @jsxImportSource solid-js */\n\
                   let Component = (props) => <ol>{props.data.map(d => <li>{d.text}</li>)}</ol>;";
        let d = crate::rules::test_helpers::run_rule(&Check, src, "t.tsx");
        assert_eq!(d.len(), 1, "solid-js pragma should fire: {d:?}");
    }
}
