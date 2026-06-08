//! OxcCheck backend for prefer-json-parse-buffer.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["readFileSync"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        // Check for JSON.parse(...)
        let Some(inner_arg) = is_json_parse(call) else { return };

        // The argument to JSON.parse should be a readFileSync call with utf8 encoding.
        let Expression::CallExpression(inner_call) = inner_arg else { return };
        if !is_readfilesync_with_utf8(inner_call, ctx.source) { return; }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer reading the JSON file as a buffer — remove the encoding argument."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn is_json_parse<'a>(call: &'a CallExpression<'a>) -> Option<&'a Expression<'a>> {
    let Expression::StaticMemberExpression(member) = &call.callee else { return None };
    let Expression::Identifier(obj) = &member.object else { return None };
    if obj.name != "JSON" || member.property.name != "parse" {
        return None;
    }
    // Exactly one argument.
    if call.arguments.len() != 1 {
        return None;
    }
    call.arguments[0].as_expression()
}

fn is_readfilesync_with_utf8(call: &CallExpression, source: &str) -> bool {
    // Accept both `readFileSync(...)` and `fs.readFileSync(...)`.
    let callee_name = match &call.callee {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    };
    if callee_name != Some("readFileSync") {
        return false;
    }
    // Need at least 2 arguments; 2nd must be utf8 encoding.
    if call.arguments.len() < 2 {
        return false;
    }
    let Some(encoding_arg) = call.arguments[1].as_expression() else {
        return false;
    };
    is_utf8_encoding_arg(encoding_arg, source)
}

fn is_utf8_encoding_arg(expr: &Expression, _source: &str) -> bool {
    match expr {
        Expression::StringLiteral(s) => {
            let inner = s.value.to_ascii_lowercase();
            inner == "utf-8" || inner == "utf8"
        }
        Expression::ObjectExpression(obj) => {
            obj.properties.iter().any(|prop| {
                let ObjectPropertyKind::ObjectProperty(p) = prop else { return false };
                let key_name = match &p.key {
                    PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
                    PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
                    _ => None,
                };
                if key_name != Some("encoding") {
                    return false;
                }
                if let Expression::StringLiteral(val) = &p.value {
                    let inner = val.value.to_ascii_lowercase();
                    inner == "utf-8" || inner == "utf8"
                } else {
                    false
                }
            })
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_oxc_ts(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_readfilesync_utf8() {
        let d = run_oxc_ts(r#"const data = JSON.parse(fs.readFileSync('config.json', 'utf-8'));"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-json-parse-buffer");
    }


    #[test]
    fn flags_readfilesync_utf8_no_dash() {
        let d = run_oxc_ts(r#"const data = JSON.parse(fs.readFileSync('config.json', 'utf8'));"#);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_readfilesync_without_encoding() {
        assert!(run_oxc_ts(r#"JSON.parse(fs.readFileSync('config.json'))"#).is_empty());
    }


    #[test]
    fn allows_non_utf8_encoding() {
        assert!(run_oxc_ts(r#"JSON.parse(fs.readFileSync('file', 'ascii'))"#).is_empty());
    }
}
