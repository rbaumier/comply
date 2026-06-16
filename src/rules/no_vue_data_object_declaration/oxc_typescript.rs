//! no-vue-data-object-declaration oxc backend for TypeScript / JavaScript / TSX.
//!
//! Inspects a Vue component options object for a `data` option declared as an
//! object literal instead of a function. The options object is taken from an
//! `export default { … }`, a `defineComponent(…)` call (last argument), or a
//! `createApp(…)` call (first argument).

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, Expression, ExportDefaultDeclarationKind, ObjectExpression, ObjectPropertyKind,
    PropertyKey,
};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Every firing path mentions a `data` option in a component object.
        Some(&["data"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for node in semantic.nodes().iter() {
            let options = match node.kind() {
                AstKind::ExportDefaultDeclaration(export) => export_options_object(&export.declaration),
                AstKind::CallExpression(call) => component_call_options_object(call),
                _ => None,
            };
            if let Some(obj) = options {
                check_options_object(obj, ctx, &mut diagnostics);
            }
        }

        diagnostics
    }
}

/// The options object of an `export default { … }`. A wrapped
/// `export default defineComponent({ … })` / `createApp({ … })` is handled via
/// the call-expression path instead, so only a bare object is taken here.
fn export_options_object<'a>(
    decl: &'a ExportDefaultDeclarationKind<'a>,
) -> Option<&'a ObjectExpression<'a>> {
    match decl.as_expression()? {
        Expression::ObjectExpression(obj) => Some(obj),
        _ => None,
    }
}

/// The options object of a `defineComponent(…)` (last argument) or
/// `createApp(…)` (first argument) call.
fn component_call_options_object<'a>(
    call: &'a oxc_ast::ast::CallExpression<'a>,
) -> Option<&'a ObjectExpression<'a>> {
    let Expression::Identifier(callee) = &call.callee else {
        return None;
    };
    let arg = match callee.name.as_str() {
        "defineComponent" => call.arguments.last()?,
        "createApp" => call.arguments.first()?,
        _ => return None,
    };
    match arg {
        Argument::ObjectExpression(obj) => Some(obj),
        _ => None,
    }
}

/// Flag a `data` option whose value is an object literal (parentheses omitted).
fn check_options_object(
    obj: &ObjectExpression,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = prop else {
            // A method (`data() {}`) is an `ObjectProperty` with `method: true`
            // in oxc, so it is handled below; spreads are skipped here.
            continue;
        };
        if prop.method {
            continue;
        }
        if !is_data_key(&prop.key) {
            continue;
        }
        if let Some(obj_value) = object_expression(&prop.value) {
            report(obj_value.span().start, ctx, diagnostics);
        }
    }
}

fn is_data_key(key: &PropertyKey) -> bool {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name == "data",
        PropertyKey::StringLiteral(s) => s.value == "data",
        _ => false,
    }
}

/// The object expression of a value, unwrapping parentheses
/// (`data: ({ … })`). A function / arrow value yields `None`.
fn object_expression<'a>(expr: &'a Expression<'a>) -> Option<&'a ObjectExpression<'a>> {
    match expr {
        Expression::ObjectExpression(obj) => Some(obj),
        Expression::ParenthesizedExpression(paren) => object_expression(&paren.expression),
        _ => None,
    }
}

fn report(span_start: u32, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: "`data` is declared as an object — declare it as a function returning the object \
                  so each component instance gets its own state."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    // --- invalid (object `data`) ---

    #[test]
    fn flags_export_default_object_data() {
        let src = "export default { data: { foo: 'bar' } };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_export_default_parenthesized_object_data() {
        let src = "export default { data: ({ foo: 'bar' }) };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_create_app_object_data() {
        let src = "createApp({ data: { foo: 'bar' } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_create_app_mount_object_data() {
        let src = "createApp({ data: { foo: 'bar' } }).mount('#app');";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_define_component_object_data() {
        let src = "export default defineComponent({ data: { foo: 'bar' } });";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- valid (function `data`) ---

    #[test]
    fn allows_method_shorthand_data() {
        let src = "export default { data() { return { foo: 'bar' }; } };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_function_data() {
        let src = "export default { data: function () { return { foo: 'bar' }; } };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_arrow_data() {
        let src = "export default { data: () => { return { foo: 'bar' }; } };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_arrow_returning_object_data() {
        let src = "export default { data: () => ({ foo: 'bar' }) };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_create_app_function_data() {
        let src = "createApp({ data: function () { return { foo: 'bar' }; } }).mount('#app');";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_method_data_with_methods() {
        let src = "export default { data() {}, methods: {} };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_data_object_nested_in_methods() {
        // A `data: {}` object inside a method body is not the component's `data`
        // option, so it must not be flagged.
        let src = "export default { methods: { foo() { const bar = { data: {} }; } } };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_new_vue_object_data() {
        // `new Vue(…)` is not a detected component shape, so its `data` is
        // never inspected.
        let src = "new Vue({ data: { foo: 'bar' } });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_plain_object_with_data() {
        // A non-component object literal is not a component options object.
        let src = "const config = { data: { foo: 'bar' } };";
        assert!(run_on(src).is_empty());
    }
}
