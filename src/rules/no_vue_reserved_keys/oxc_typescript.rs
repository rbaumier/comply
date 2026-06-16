//! no-vue-reserved-keys oxc backend for TypeScript / JavaScript / TSX.
//!
//! Inspects a Vue component options object (`export default { … }`, optionally
//! wrapped in `defineComponent(…)` / `Vue.extend(…)`) and `<script setup>`
//! `defineProps(…)` / `defineProps<…>()` for Vue-reserved keys.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, Expression, ExportDefaultDeclarationKind, ObjectExpression, ObjectPropertyKind,
    PropertyKey, Statement, TSSignature, TSType, TSTypeName,
};
use oxc_span::GetSpan;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Vue instance properties reserved with a `$` prefix. Declaring one of these
/// as an option key shadows the framework member. Sorted for `binary_search`.
const RESERVED_KEYS: &[&str] = &[
    "$attrs",
    "$children",
    "$data",
    "$delete",
    "$destroy",
    "$el",
    "$emit",
    "$forceUpdate",
    "$isServer",
    "$listeners",
    "$mount",
    "$nextTick",
    "$off",
    "$on",
    "$once",
    "$options",
    "$parent",
    "$props",
    "$refs",
    "$root",
    "$scopedSlots",
    "$set",
    "$slots",
    "$watch",
];

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        // Every firing path goes through one of these identifiers.
        Some(&["export default", "defineProps"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        // Pass 1: index `interface` / `type` members so a `defineProps<Name>()`
        // can resolve its reserved keys.
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
                        check_options_object(obj, ctx, &mut diagnostics);
                    }
                }
                AstKind::CallExpression(call) => {
                    if !is_define_props(&call.callee) {
                        continue;
                    }
                    // `defineProps({ … })` — every key is checked as a prop name.
                    if let Some(Argument::ObjectExpression(obj)) = call.arguments.first() {
                        for (name, span_start) in object_keys(obj) {
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
                _ => {}
            }
        }

        diagnostics
    }
}

/// The component options object from an `export default`, unwrapping a single
/// `defineComponent(…)` / `Vue.extend(…)` call wrapper.
fn options_object<'a>(
    decl: &'a ExportDefaultDeclarationKind<'a>,
) -> Option<&'a ObjectExpression<'a>> {
    let expr = decl.as_expression()?;
    match expr {
        Expression::ObjectExpression(obj) => Some(obj),
        Expression::CallExpression(call) if is_component_wrapper(&call.callee) => {
            match call.arguments.first()? {
                Argument::ObjectExpression(obj) => Some(obj),
                _ => None,
            }
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

fn is_define_props(callee: &Expression) -> bool {
    matches!(callee, Expression::Identifier(id) if id.name == "defineProps")
}

/// Walk the option keys (`data`, `computed`, `methods`, `props`, `asyncData`)
/// of an options object and report reserved keys in each collection.
fn check_options_object(
    obj: &ObjectExpression,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for prop in &obj.properties {
        let ObjectPropertyKind::ObjectProperty(prop) = prop else {
            continue;
        };
        let Some(option) = static_key_name(&prop.key) else {
            continue;
        };
        match option {
            // `data` / `asyncData`: `$`-reserved AND `_`-prefixed keys are flagged.
            "data" | "asyncData" => {
                for (name, span_start) in data_return_keys(&prop.value) {
                    if RESERVED_KEYS.binary_search(&name).is_ok() || name.starts_with('_') {
                        report(name, span_start, ctx, diagnostics);
                    }
                }
            }
            // `computed` / `methods`: only `$`-reserved keys are flagged.
            "computed" | "methods" => {
                if let Expression::ObjectExpression(inner) = &prop.value {
                    for (name, span_start) in object_keys(inner) {
                        report_reserved(name, span_start, ctx, diagnostics);
                    }
                }
            }
            // `props`: an array of string names OR an object; `$`-reserved only.
            "props" => {
                for (name, span_start) in props_keys(&prop.value) {
                    report_reserved(name, span_start, ctx, diagnostics);
                }
            }
            _ => {}
        }
    }
}

/// Keys of a `data` / `asyncData` value: an object literal, or a function /
/// arrow returning an object (including the `() => ({ … })` short form).
fn data_return_keys<'a>(value: &'a Expression<'a>) -> Vec<(&'a str, u32)> {
    match value {
        Expression::ObjectExpression(obj) => object_keys(obj),
        Expression::FunctionExpression(func) => func
            .body
            .as_ref()
            .map(|body| returned_object_keys(&body.statements))
            .unwrap_or_default(),
        Expression::ArrowFunctionExpression(arrow) => {
            if arrow.expression {
                // `() => ({ … })` — the body is a single expression statement.
                arrow
                    .body
                    .statements
                    .first()
                    .and_then(|stmt| match stmt {
                        Statement::ExpressionStatement(es) => {
                            object_expression(&es.expression).map(object_keys)
                        }
                        _ => None,
                    })
                    .unwrap_or_default()
            } else {
                returned_object_keys(&arrow.body.statements)
            }
        }
        _ => Vec::new(),
    }
}

/// Keys of the object in the first `return <object>;` of a statement list.
fn returned_object_keys<'a>(statements: &'a [Statement<'a>]) -> Vec<(&'a str, u32)> {
    for stmt in statements {
        if let Statement::ReturnStatement(ret) = stmt {
            if let Some(arg) = &ret.argument {
                if let Some(obj) = object_expression(arg) {
                    return object_keys(obj);
                }
            }
        }
    }
    Vec::new()
}

/// Keys of a `props` value: an array of string literals or an object literal.
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
            named_types.get(ident.name.as_str()).cloned().unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

fn object_expression<'a>(expr: &'a Expression<'a>) -> Option<&'a ObjectExpression<'a>> {
    match expr {
        Expression::ObjectExpression(obj) => Some(obj),
        Expression::ParenthesizedExpression(paren) => object_expression(&paren.expression),
        _ => None,
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
    if RESERVED_KEYS.binary_search(&name).is_ok() {
        report(name, span_start, ctx, diagnostics);
    }
}

fn report(name: &str, span_start: u32, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: format!("`{name}` is a Vue-reserved key — rename it to avoid shadowing Vue's instance member."),
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

    #[test]
    fn flags_data_object_reserved_and_underscore() {
        let src = "export default { data: { $el: '', _foo: String } };";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn flags_props_array() {
        let src = "export default { props: ['$el'] };";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_define_component_wrapper() {
        let src = "export default defineComponent({ props: { $el: String } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_vue_extend_wrapper() {
        let src = "export default Vue.extend({ methods: { $emit() {} } });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_underscore_in_methods() {
        let src = "export default { methods: { _foo() {} } };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_plain_keys() {
        let src = "export default { data() { return { message: 'hi', count: 0 }; } };";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_component_object() {
        // A non-export object literal is not a component options object.
        let src = "const config = { data: { $el: 1 } };";
        assert!(run_on(src).is_empty());
    }
}
