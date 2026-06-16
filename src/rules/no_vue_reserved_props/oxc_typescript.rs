//! no-vue-reserved-props oxc backend for TypeScript / JavaScript / TSX.
//!
//! Inspects the `props` of a Vue component options object (`export default { … }`,
//! optionally wrapped in `defineComponent(…)` / `Vue.extend(…)`, and the
//! `defineComponent(setup, { props })` two-arg form), a `createApp({ props })`
//! root component, and `<script setup>` `defineProps(…)` / `defineProps<…>()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, Expression, ExportDefaultDeclarationKind, ObjectExpression, ObjectPropertyKind,
    PropertyKey, TSSignature, TSType, TSTypeName,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Prop names Vue reserves for template binding. Declaring either as a prop
/// shadows the framework attribute. Sorted for `binary_search`.
const RESERVED_PROPS: &[&str] = &["key", "ref"];

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Every firing path goes through one of these identifiers.
        Some(&["export default", "defineProps", "createApp"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        // Pass 1: index `interface` / `type` members so a `defineProps<Name>()`
        // can resolve its reserved props.
        let mut named_types: FxHashMap<&str, Vec<(&str, u32)>> = FxHashMap::default();
        for node in nodes.iter() {
            match node.kind() {
                AstKind::TSInterfaceDeclaration(decl) => {
                    named_types.insert(decl.id.name.as_str(), signature_keys(&decl.body.body));
                }
                AstKind::TSTypeAliasDeclaration(decl) => {
                    if let TSType::TSTypeLiteral(lit) = &decl.type_annotation {
                        named_types.insert(decl.id.name.as_str(), signature_keys(&lit.members));
                    }
                }
                _ => {}
            }
        }

        for node in nodes.iter() {
            match node.kind() {
                AstKind::ExportDefaultDeclaration(export) => {
                    if let Some(obj) = options_object(&export.declaration) {
                        check_props_in_options(obj, ctx, &mut diagnostics);
                    }
                }
                AstKind::CallExpression(call) => {
                    if is_create_app(&call.callee) {
                        // `createApp({ props: … })` — the root component options.
                        if let Some(Argument::ObjectExpression(obj)) = call.arguments.first() {
                            check_props_in_options(obj, ctx, &mut diagnostics);
                        }
                    } else if is_define_props(&call.callee) {
                        // `defineProps({ … })` / `defineProps([ … ])` — every key
                        // or string element is a prop name.
                        if let Some(arg) = call.arguments.first().and_then(Argument::as_expression) {
                            for (name, span_start) in props_keys(arg) {
                                report_reserved(name, span_start, ctx, &mut diagnostics);
                            }
                        }
                        // `defineProps<T>()` — resolve the type argument's keys.
                        if let Some(type_args) = call.type_arguments.as_ref() {
                            for (name, span_start) in type_arg_keys(type_args, &named_types) {
                                report_reserved(name, span_start, ctx, &mut diagnostics);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        diagnostics
    }
}

/// The component options object from an `export default`, unwrapping a single
/// `defineComponent(…)` / `Vue.extend(…)` call wrapper. For the
/// `defineComponent(setup, { props })` form the options are the second argument.
fn options_object<'a>(
    decl: &'a ExportDefaultDeclarationKind<'a>,
) -> Option<&'a ObjectExpression<'a>> {
    let expr = decl.as_expression()?;
    match expr {
        Expression::ObjectExpression(obj) => Some(obj),
        Expression::CallExpression(call) if is_component_wrapper(&call.callee) => {
            call.arguments.iter().find_map(|arg| match arg {
                Argument::ObjectExpression(obj) => Some(obj.as_ref()),
                _ => None,
            })
        }
        _ => None,
    }
}

/// True for `defineComponent` or `Vue.extend` — the common Options-API wrappers.
fn is_component_wrapper(callee: &Expression) -> bool {
    match callee {
        Expression::Identifier(id) => id.name == "defineComponent",
        Expression::StaticMemberExpression(member) => member.property.name == "extend",
        _ => false,
    }
}

fn is_create_app(callee: &Expression) -> bool {
    matches!(callee, Expression::Identifier(id) if id.name == "createApp")
}

fn is_define_props(callee: &Expression) -> bool {
    matches!(callee, Expression::Identifier(id) if id.name == "defineProps")
}

/// Report reserved prop names declared in the `props` option of an options object.
fn check_props_in_options(
    obj: &ObjectExpression,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = prop else {
            continue;
        };
        if static_key_name(&prop.key) != Some("props") {
            continue;
        }
        for (name, span_start) in props_keys(&prop.value) {
            report_reserved(name, span_start, ctx, diagnostics);
        }
    }
}

/// Prop names of a `props` value: an array of string literals or an object literal.
fn props_keys<'a>(value: &'a Expression<'a>) -> Vec<(&'a str, u32)> {
    match value {
        Expression::ObjectExpression(obj) => object_keys(obj),
        Expression::ArrayExpression(arr) => arr
            .elements
            .iter()
            .filter_map(|el| match el.as_expression()? {
                Expression::StringLiteral(s) => Some((s.value.as_str(), s.span.start)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Static (identifier / string-literal) keys of an object literal, with the
/// byte offset of each key for diagnostics.
fn object_keys<'a>(obj: &'a ObjectExpression<'a>) -> Vec<(&'a str, u32)> {
    obj.properties
        .iter()
        .filter_map(|prop| {
            let ObjectPropertyKind::ObjectProperty(prop) = prop else {
                return None;
            };
            let name = static_key_name(&prop.key)?;
            Some((name, prop.key.span().start))
        })
        .collect()
}

/// Property keys of `interface` / `type` members, with their byte offsets.
fn signature_keys<'a>(sigs: &'a [TSSignature<'a>]) -> Vec<(&'a str, u32)> {
    sigs.iter()
        .filter_map(|sig| {
            let TSSignature::TSPropertySignature(prop) = sig else {
                return None;
            };
            let name = static_key_name(&prop.key)?;
            Some((name, prop.key.span().start))
        })
        .collect()
}

/// Keys resolved from a `defineProps<…>()` type argument — either an inline
/// type literal or a reference to a known `interface` / `type`.
fn type_arg_keys<'a>(
    type_args: &'a oxc_ast::ast::TSTypeParameterInstantiation<'a>,
    named_types: &FxHashMap<&'a str, Vec<(&'a str, u32)>>,
) -> Vec<(&'a str, u32)> {
    let Some(first) = type_args.params.first() else {
        return Vec::new();
    };
    match first {
        TSType::TSTypeLiteral(lit) => signature_keys(&lit.members),
        TSType::TSTypeReference(tref) => {
            let TSTypeName::IdentifierReference(ident) = &tref.type_name else {
                return Vec::new();
            };
            named_types
                .get(ident.name.as_str())
                .cloned()
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

fn static_key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

fn report_reserved(name: &str, span_start: u32, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    if RESERVED_PROPS.binary_search(&name).is_ok() {
        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{name}` is a Vue-reserved attribute and cannot be used as a prop name."
            ),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_export_default_props_object() {
        let src = "export default { props: { ref: String, key: String, foo: String } };";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn flags_export_default_props_array() {
        let src = "export default { props: ['ref', 'key', 'foo'] };";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn flags_define_component_wrapper() {
        let src = "export default defineComponent({ props: ['ref', 'key', 'foo'] });";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn flags_define_component_setup_two_arg() {
        let src = "export default defineComponent((props) => {}, { props: ['ref', 'key', 'foo'] });";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn flags_create_app() {
        let src = "createApp({ props: ['ref', 'key', 'foo'] }).mount('#app');";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_non_reserved_props() {
        let src = "export default { props: { foo: String, message: String } };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_reserved_name_outside_props() {
        // `ref` in `data` is not a prop declaration.
        let src = "export default { data: { ref: '' } };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_component_object() {
        let src = "const config = { props: { ref: 1 } };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_create_app_with_component_import() {
        let src = "createApp(MyComponent).mount('#app');";
        assert!(run_on(src).is_empty());
    }
}
