//! OXC backend for elysia-t-unknown-format-string — flag unrecognised
//! `format` values in `t.String({ format: '...' })`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

const KNOWN_FORMATS: &[&str] = &[
    "email",
    "uri",
    "uuid",
    "date",
    "date-time",
    "ipv4",
    "ipv6",
    "hostname",
    "regex",
    "time",
    "duration",
    "json-pointer",
    "relative-json-pointer",
    "uri-reference",
    "uri-template",
    "iri",
    "iri-reference",
    "idn-email",
    "idn-hostname",
];

pub struct Check;

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
        let AstKind::CallExpression(call) = node.kind() else { return };

        if !ctx.project.has_framework("elysia") {
            return;
        }

        // Callee must be `t.String`
        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "t" || member.property.name.as_str() != "String" {
            return;
        }

        // Look for an object argument with `format: 'value'`
        for arg in &call.arguments {
            let Argument::ObjectExpression(obj) = arg else { continue };
            for prop in &obj.properties {
                let ObjectPropertyKind::ObjectProperty(p) = prop else { continue };
                let key_name = match &p.key {
                    PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                    PropertyKey::StringLiteral(s) => s.value.as_str(),
                    _ => continue,
                };
                if key_name != "format" {
                    continue;
                }
                let Expression::StringLiteral(val) = &p.value else { continue };
                let format_str = val.value.as_str();
                if KNOWN_FORMATS.contains(&format_str) {
                    continue;
                }
                let (line, column) = byte_offset_to_line_col(ctx.source, p.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: "elysia-t-unknown-format-string".into(),
                    message: format!("`format: '{format_str}'` is not a recognised JSON-schema format \u{2014} TypeBox will silently skip the check."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_unknown_format() {
        let src = "import { t } from 'elysia';\nconst s = t.String({ format: 'emial' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_known_format_email() {
        let src = "import { t } from 'elysia';\nconst s = t.String({ format: 'email' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_known_format_date_time() {
        let src = "import { t } from 'elysia';\nconst s = t.String({ format: 'date-time' });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_string_without_format() {
        let src = "import { t } from 'elysia';\nconst s = t.String({ minLength: 1 });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const s = t.String({ format: 'emial' });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
