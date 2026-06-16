//! use-unique-element-ids oxc backend.
//!
//! Ports Biome's `useUniqueElementIds`. Two shapes carry a static element id:
//!
//! - a JSX `id` attribute whose value is a plain string literal
//!   (`<div id="foo">`). A dynamic `id={x}` is fine.
//! - a React `createElement(tag, { id: <literal> })` call. The call counts as
//!   React's `createElement` when the callee is `React.createElement` or a bare
//!   `createElement` imported from `react`.
//!
//! An element whose unqualified name is listed in `excluded_components` (e.g.
//! `FormattedMessage`, which uses `id` for an i18n message, not a DOM id) is
//! skipped on both shapes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Expression, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, JSXElementName,
    ObjectPropertyKind, PropertyKey,
};
use rustc_hash::FxHashSet;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        // React-only advice (`useId()`); skip Solid/Vue/Preact/Qwik JSX, which
        // do not reuse host ids the same way and have no `useId`.
        if crate::oxc_helpers::is_non_react_jsx_file(ctx.source, ctx.project, ctx.path) {
            return Vec::new();
        }

        let excluded = ctx
            .config
            .string_list(super::META.id, "excluded_components", ctx.lang);
        let is_excluded = |name: &str| excluded.iter().any(|e| e == name);

        // Bare `createElement(...)` only counts when imported from `react`.
        let bare_create_element = react_create_element_bindings(semantic);

        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            match node.kind() {
                AstKind::JSXOpeningElement(opening) => {
                    if jsx_element_name(&opening.name).is_some_and(&is_excluded) {
                        continue;
                    }
                    if let Some(span) = static_jsx_id_span(opening) {
                        diagnostics.push(self.diag(ctx, span));
                    }
                }
                AstKind::CallExpression(call) => {
                    if !is_react_create_element(call, &bare_create_element) {
                        continue;
                    }
                    if create_element_name(call).is_some_and(&is_excluded) {
                        continue;
                    }
                    if let Some(span) = literal_id_prop_span(call) {
                        diagnostics.push(self.diag(ctx, span));
                    }
                }
                _ => {}
            }
        }
        diagnostics
    }
}

impl Check {
    fn diag(&self, ctx: &CheckCtx, span_start: u32) -> Diagnostic {
        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Avoid a static `id` on a JSX element. A reused component \
                      renders duplicate ids; use `useId()` and pass `id={id}`."
                .into(),
            severity: Severity::Warning,
            span: None,
        }
    }
}

/// The unqualified name of a JSX element, used for the `excluded_components`
/// check. A member (`Library.FormattedMessage`) or namespaced
/// (`svg:FormattedMessage`) name resolves to its last segment, matching Biome's
/// `name_value_token`.
fn jsx_element_name<'a>(name: &'a JSXElementName<'a>) -> Option<&'a str> {
    match name {
        JSXElementName::Identifier(id) => Some(id.name.as_str()),
        JSXElementName::IdentifierReference(id) => Some(id.name.as_str()),
        JSXElementName::MemberExpression(member) => Some(member.property.name.as_str()),
        JSXElementName::NamespacedName(ns) => Some(ns.name.name.as_str()),
        JSXElementName::ThisExpression(_) => None,
    }
}

/// The span of a static string-literal `id` attribute, or `None` when the
/// element has no `id`, a dynamic `id={x}`, or an expression-container value.
fn static_jsx_id_span(opening: &oxc_ast::ast::JSXOpeningElement) -> Option<u32> {
    for attr_item in &opening.attributes {
        let JSXAttributeItem::Attribute(attr) = attr_item else {
            continue;
        };
        let JSXAttributeName::Identifier(name_ident) = &attr.name else {
            continue;
        };
        if name_ident.name.as_str() != "id" {
            continue;
        }
        if let Some(JSXAttributeValue::StringLiteral(_)) = &attr.value {
            return Some(attr.span.start);
        }
        return None;
    }
    None
}

/// True when `call` is React's `createElement` — either `React.createElement(…)`
/// or a bare `createElement(…)` whose binding was imported from `react`.
fn is_react_create_element(
    call: &oxc_ast::ast::CallExpression,
    bare_bindings: &FxHashSet<String>,
) -> bool {
    match &call.callee {
        Expression::StaticMemberExpression(member) => {
            member.property.name.as_str() == "createElement"
                && matches!(&member.object, Expression::Identifier(id) if id.name.as_str() == "React")
        }
        Expression::Identifier(id) => bare_bindings.contains(id.name.as_str()),
        _ => false,
    }
}

/// The component name passed as `createElement`'s first argument, for the
/// `excluded_components` check. Only an identifier (`FormattedMessage`) or a
/// member's last segment (`Library.FormattedMessage`) yields a name — matching
/// Biome's `get_callee_member_name`. A string-literal element name
/// (`createElement("FormattedMessage", …)`) yields `None`, so it is never
/// excluded and stays flagged.
fn create_element_name<'a>(call: &'a oxc_ast::ast::CallExpression<'a>) -> Option<&'a str> {
    match call.arguments.first()?.as_expression()? {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    }
}

/// The span of a literal-valued `id` property in `createElement`'s second
/// (props) argument, or `None`. A shorthand (`{ id }`) or computed/spread/method
/// member is not a static literal and is ignored.
fn literal_id_prop_span(call: &oxc_ast::ast::CallExpression) -> Option<u32> {
    let Expression::ObjectExpression(obj) = call.arguments.get(1)?.as_expression()? else {
        return None;
    };
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(p) = prop else {
            continue;
        };
        let key = match &p.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if key != "id" {
            continue;
        }
        if p.shorthand {
            return None;
        }
        if is_literal_expression(&p.value) {
            return Some(p.span.start);
        }
        return None;
    }
    None
}

/// True for an `AnyJsLiteralExpression` (string, number, bool, null, bigint,
/// regex) — the set Biome flags for the object-member `id`.
fn is_literal_expression(expr: &Expression) -> bool {
    matches!(
        expr,
        Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::RegExpLiteral(_)
    )
}

/// Local names bound to `react`'s `createElement` via a named import
/// (`import { createElement } from "react"`, or `… as h`). A `not-react` source
/// is excluded, matching Biome's React-API resolution.
fn react_create_element_bindings(semantic: &oxc_semantic::Semantic) -> FxHashSet<String> {
    let mut bindings = FxHashSet::default();
    for node in semantic.nodes().iter() {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            continue;
        };
        if import.source.value.as_str() != "react" {
            continue;
        }
        let Some(specifiers) = &import.specifiers else {
            continue;
        };
        for spec in specifiers {
            if let oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(named) = spec
                && named.imported.name().as_str() == "createElement"
            {
                bindings.insert(named.local.name.to_string());
            }
        }
    }
    bindings
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    // ---- Biome invalid.jsx fixtures ----

    #[test]
    fn flags_static_id_on_jsx_element() {
        let src = "function Foo() { return <div id=\"foo\"></div>; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_static_id_on_parent_with_child() {
        let src = "function Foo() { return (<div id=\"foo\"><div>bar</div></div>); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_static_id_on_nested_child() {
        let src = "function Foo() { return (<div><div id=\"foo\">bar</div></div>); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_react_create_element_member() {
        let src = "function Foo() { return React.createElement(\"div\", { id: \"foo\" }); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_bare_create_element_imported_from_react() {
        let src = "import { createElement } from \"react\";\n\
                   function Foo() { return createElement(\"div\", { id: \"foo\" }); }";
        assert_eq!(run(src).len(), 1);
    }

    // ---- Biome valid.jsx fixtures ----

    #[test]
    fn allows_dynamic_id_from_useid() {
        let src = "function Foo() { const id = useId(); return <div id={id}></div>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_dynamic_id_nested() {
        let src =
            "function Foo() { const id = useId(); return (<div id={id}><div>bar</div></div>); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_dynamic_id_from_crypto() {
        let src = "function Foo() { const id = crypto.randomUUID(); return <div id={id}></div>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_id_from_prop() {
        let src = "function Foo({ id }) { return <div id={id}></div>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_create_element_shorthand_id() {
        let src =
            "function Foo() { const id = useId(); return React.createElement(\"div\", { id }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_create_element_without_react_import() {
        // No `createElement` import at all — not React's createElement.
        let src = "function Foo() { return createElement(\"div\", { id: \"foo\" }); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_bare_create_element_from_non_react() {
        let src = "import { createElement } from \"not-react\";\n\
                   function Foo() { return createElement(\"div\", { id: \"foo\" }); }";
        assert!(run(src).is_empty());
    }

    // ---- Biome allowlist.jsx fixtures (excluded_components = ["FormattedMessage"]) ----

    /// Run the rule with `excluded_components = ["FormattedMessage"]` by
    /// building a real `Config` from a temp `comply.toml` and threading it
    /// through a hand-built `CheckCtx` (the standard helpers use the default
    /// config, which carries no excludes).
    fn run_excluded(src: &str) -> Vec<Diagnostic> {
        use crate::config::Config;
        use crate::files::Language;
        use crate::rules::backend::CheckCtx;
        use oxc_allocator::Allocator;
        use oxc_parser::Parser as OxcParser;
        use oxc_semantic::SemanticBuilder;
        use std::path::Path;

        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("comply.toml"),
            "[rules.use-unique-element-ids]\nexcluded_components = [\"FormattedMessage\"]\n",
        )
        .unwrap();
        let config = Config::load_from(tmp.path()).unwrap();

        crate::oxc_helpers::reset_file_caches();
        let path = Path::new("t.tsx");
        let source_type = crate::oxc_helpers::source_type_for_path(path);
        let allocator = Allocator::default();
        let parse_ret = OxcParser::new(&allocator, src, source_type).parse();
        let semantic = SemanticBuilder::new().build(&parse_ret.program).semantic;

        let project = crate::project::default_static_project_ctx();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        let ctx = CheckCtx {
            path,
            path_arc: std::sync::Arc::from(path),
            source: src,
            config: &config,
            project,
            file,
            lang: Language::Tsx,
        };
        Check.run_on_semantic(&semantic, &ctx)
    }

    #[test]
    fn excluded_component_jsx_is_allowed() {
        assert!(run_excluded("function W() { return <FormattedMessage id=\"abc\"></FormattedMessage>; }").is_empty());
    }

    #[test]
    fn excluded_component_jsx_self_closing_is_allowed() {
        assert!(run_excluded("function W() { return <FormattedMessage id=\"abc\"/>; }").is_empty());
    }

    #[test]
    fn excluded_component_jsx_namespaced_is_allowed() {
        assert!(
            run_excluded("function W() { return <Library.FormattedMessage id=\"abc\"/>; }")
                .is_empty()
        );
    }

    #[test]
    fn excluded_component_create_element_is_allowed() {
        assert!(
            run_excluded("function W() { return React.createElement(FormattedMessage, {id: \"abc\"}); }")
                .is_empty()
        );
    }

    #[test]
    fn excluded_component_create_element_namespaced_is_allowed() {
        assert!(
            run_excluded("function W() { return React.createElement(Library.FormattedMessage, {id: \"abc\"}); }")
                .is_empty()
        );
    }

    #[test]
    fn non_excluded_component_jsx_still_flagged() {
        assert_eq!(
            run_excluded("function W() { return <OtherFormattedMessage id=\"abc\"></OtherFormattedMessage>; }").len(),
            1
        );
    }

    #[test]
    fn non_excluded_component_create_element_still_flagged() {
        assert_eq!(
            run_excluded("function W() { return React.createElement(OtherFormattedMessage, {id: \"abc\"}); }").len(),
            1
        );
    }

    #[test]
    fn string_element_name_not_matched_against_excluded_list() {
        // `"FormattedMessage"` as a string element name is the element being
        // created, not the excluded component identifier — Biome still flags it.
        assert_eq!(
            run_excluded("function W() { return React.createElement(\"FormattedMessage\", {id: \"abc\"}); }").len(),
            1
        );
    }

    // ---- extra edge coverage ----

    #[test]
    fn allows_string_id_in_solid_file() {
        let src = "import { createSignal } from \"solid-js\";\nconst x = <div id=\"foo\"></div>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_static_id_on_component_element() {
        // A non-host (component) element is in scope too, matching Biome.
        let src = "function Foo() { return <Widget id=\"foo\"/>; }";
        assert_eq!(run(src).len(), 1);
    }
}
