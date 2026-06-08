//! prefer-prototype-methods oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const OBJECT_PATTERNS: &[(&str, &str, &str)] = &[
    ("hasOwnProperty", "Object", "Object.prototype.hasOwnProperty"),
    ("isPrototypeOf", "Object", "Object.prototype.isPrototypeOf"),
    ("propertyIsEnumerable", "Object", "Object.prototype.propertyIsEnumerable"),
    ("toLocaleString", "Object", "Object.prototype.toLocaleString"),
    ("toString", "Object", "Object.prototype.toString"),
    ("valueOf", "Object", "Object.prototype.valueOf"),
];

const ARRAY_PATTERNS: &[(&str, &str, &str)] = &[
    ("slice", "Array", "Array.prototype.slice"),
    ("map", "Array", "Array.prototype.map"),
    ("forEach", "Array", "Array.prototype.forEach"),
    ("filter", "Array", "Array.prototype.filter"),
    ("concat", "Array", "Array.prototype.concat"),
    ("indexOf", "Array", "Array.prototype.indexOf"),
    ("join", "Array", "Array.prototype.join"),
    ("push", "Array", "Array.prototype.push"),
    ("splice", "Array", "Array.prototype.splice"),
    ("reduce", "Array", "Array.prototype.reduce"),
    ("find", "Array", "Array.prototype.find"),
    ("includes", "Array", "Array.prototype.includes"),
    ("some", "Array", "Array.prototype.some"),
    ("every", "Array", "Array.prototype.every"),
    ("flat", "Array", "Array.prototype.flat"),
    ("flatMap", "Array", "Array.prototype.flatMap"),
];

const DELEGATION: &[&str] = &["call", "apply", "bind"];

pub struct Check;

/// Unwrap a ParenthesizedExpression wrapper.
fn unwrap_parens<'a>(expr: &'a Expression<'a>) -> &'a Expression<'a> {
    if let Expression::ParenthesizedExpression(p) = expr {
        unwrap_parens(&p.expression)
    } else {
        expr
    }
}

/// Check if expression is an empty object `{}`.
fn is_empty_object(expr: &Expression) -> bool {
    let expr = unwrap_parens(expr);
    matches!(expr, Expression::ObjectExpression(obj) if obj.properties.is_empty())
}

/// Check if expression is an empty array `[]`.
fn is_empty_array(expr: &Expression) -> bool {
    let expr = unwrap_parens(expr);
    matches!(expr, Expression::ArrayExpression(arr) if arr.elements.is_empty())
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `<inner>.<delegation>` where delegation is call/apply/bind.
        let Expression::StaticMemberExpression(outer) = &call.callee else {
            return;
        };
        let delegation = outer.property.name.as_str();
        if !DELEGATION.contains(&delegation) {
            return;
        }

        // The object of the outer must also be a static member: `<literal>.<method>`.
        let Expression::StaticMemberExpression(inner) = &outer.object else {
            return;
        };
        let method = inner.property.name.as_str();

        let is_object = is_empty_object(&inner.object);
        let is_array = is_empty_array(&inner.object);

        let patterns: &[(&str, &str, &str)] = if is_object {
            OBJECT_PATTERNS
        } else if is_array {
            ARRAY_PATTERNS
        } else {
            return;
        };

        let Some((_method, constructor, replacement)) =
            patterns.iter().find(|(m, _, _)| *m == method)
        else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Prefer `{replacement}.{delegation}(\u{2026})` over borrowing from a literal instance. \
                 Use `{constructor}.prototype.{method}` instead."
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
    fn flags_object_has_own_property_call() {
        let d = run_on("const has = ({}).hasOwnProperty.call(obj, 'key');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object.prototype.hasOwnProperty"));
    }

    #[test]
    fn flags_object_to_string_call() {
        let d = run_on("const type = ({}).toString.call(value);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object.prototype.toString"));
    }

    #[test]
    fn flags_array_slice_call() {
        let d = run_on("const args = [].slice.call(arguments);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.prototype.slice"));
    }

    #[test]
    fn flags_array_map_call() {
        let d = run_on("[].map.call(nodeList, fn)");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.prototype.map"));
    }

    #[test]
    fn allows_prototype_methods() {
        assert!(run_on("Object.prototype.hasOwnProperty.call(obj, 'key')").is_empty());
    }

    #[test]
    fn allows_normal_method_calls() {
        assert!(run_on("obj.hasOwnProperty('key')").is_empty());
    }
}
