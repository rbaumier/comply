//! react-jsx-no-jsx-as-prop oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXExpression};
use std::sync::Arc;

const ALLOWED_PROPS: &[&str] = &[
    "trigger",
    "content",
    "icon",
    "overlay",
    "asChild",
    "fallback",
    "label",
    "description",
    "title",
    "action",
    "prefix",
    "suffix",
    "left",
    "right",
    "header",
    "footer",
    // React Router v6 composition slots: `element` is the route's render
    // target and `children` nests routes — both receive a JSX literal by
    // design. React Router caches the element reference, so the
    // "new element every render" concern does not apply.
    "element",
    "children",
    // Base UI / Radix / coss composition API: a primitive accepts a
    // JSX element in `render` and calls cloneElement on it to merge
    // its own props onto the consumer's element. JSX literal is the
    // intended shape; "extract to a variable" doesn't save anything.
    "render",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXOpeningElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXOpeningElement(opening) = node.kind() else {
            return;
        };

        // SolidJS compiles JSX to DOM-creation expressions that run once; there
        // is no component re-render cycle re-evaluating prop values, so JSX as a
        // prop value carries no referential-equality cost. Gate on `!imports_solid`
        // (not `imports_react`) so React `.tsx` files using the new JSX transform
        // without an explicit `import React` are still covered.
        if crate::oxc_helpers::imports_solid(ctx.source) {
            return;
        }

        for attr_item in &opening.attributes {
            let JSXAttributeItem::Attribute(attr) = attr_item else {
                continue;
            };
            let JSXAttributeName::Identifier(attr_ident) = &attr.name else {
                continue;
            };
            let attr_name = attr_ident.name.as_str();
            if ALLOWED_PROPS.contains(&attr_name) {
                continue;
            }

            let Some(JSXAttributeValue::ExpressionContainer(ec)) = &attr.value else {
                continue;
            };

            let kind_label = match &ec.expression {
                JSXExpression::EmptyExpression(_) => continue,
                JSXExpression::JSXElement(_) => "JSX element",
                JSXExpression::JSXFragment(_) => "JSX fragment",
                _ => continue,
            };

            let (line, column) =
                byte_offset_to_line_col(ctx.source, ec.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "{kind_label} as value of JSX prop `{attr_name}` creates a new element every render — extract to a variable or `useMemo`."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
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
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_jsx_as_unknown_prop() {
        let src = r#"const x = <Wrapper before={<Inner />} />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_render_prop_base_ui_composition() {
        // Regression for rbaumier/comply#17 — Base UI's `render` prop
        // expects a JSX element and is the documented composition API.
        let src = r#"const x = <DropdownMenuItem render={<Link to="/account" />}>Mon compte</DropdownMenuItem>;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_known_slot_props() {
        let src = r#"const x = <Card header={<Title />} footer={<Buttons />} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_react_router_element_prop() {
        // Regression for rbaumier/comply#1356 — React Router v6's `element`
        // prop is a composition slot designed to receive a JSX element.
        let src = r#"const x = <Route element={<HomePage />} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_react_router_nested_element_prop() {
        let src = r#"const x = <Route element={<Authenticated><Outlet /></Authenticated>} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_children_jsx_prop() {
        let src = r#"const x = <Provider children={<App />} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_non_allowed_prop_with_jsx_value() {
        // Negative-space guard: a prop outside the allowlist still fires.
        let src = r#"const x = <Wrapper foo={<Bar />} />;"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_solidjs_jsx_as_prop() {
        // Regression for rbaumier/comply#3218 — SolidJS has fine-grained
        // reactivity (the component body runs once), so JSX as a prop value
        // carries no re-render cost. Files importing solid-js are not flagged.
        let src = r#"import { createSignal } from "solid-js";
const x = <Comp assets={<HydrationScript />} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_solidjs_start_jsx_as_prop() {
        let src = r#"import { StartServer } from "@solidjs/start/server";
const x = <Comp assets={<HydrationScript />} />;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_react_jsx_as_prop_without_explicit_react_import() {
        // `!imports_solid` (not `imports_react`) preserves the core case: a
        // React `.tsx` using the new JSX transform has no `import React`, yet
        // the referential-equality concern still applies.
        let src = r#"const x = <Comp assets={<Child />} />;"#;
        assert_eq!(run(src).len(), 1);
    }
}

