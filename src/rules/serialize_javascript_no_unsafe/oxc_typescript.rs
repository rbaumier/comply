use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["serialize"])
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
        let Expression::Identifier(ident) = &call.callee else {
            return;
        };
        if ident.name.as_str() != "serialize" {
            return;
        }
        if call.arguments.len() < 2 {
            return;
        }
        let Some(arg_expr) = call.arguments[1].as_expression() else {
            return;
        };
        let Expression::ObjectExpression(obj) = arg_expr else {
            return;
        };

        for prop_or_spread in &obj.properties {
            let ObjectPropertyKind::ObjectProperty(prop) = prop_or_spread else {
                continue;
            };
            let key_name = match &prop.key {
                PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                PropertyKey::StringLiteral(s) => s.value.as_str(),
                _ => continue,
            };
            if key_name != "unsafe" {
                continue;
            }
            let is_true = matches!(&prop.value, Expression::BooleanLiteral(b) if b.value);
            if !is_true {
                continue;
            }
            let (line, column) =
                byte_offset_to_line_col(ctx.source, prop.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`serialize(..., { unsafe: true })` disables HTML escaping — remove the `unsafe` option.".into(),
                severity: Severity::Error,
                span: None,
            });
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_unsafe_true() {
        assert_eq!(run("serialize(data, { unsafe: true })").len(), 1);
    }


    #[test]
    fn flags_unsafe_true_quoted_key() {
        assert_eq!(run(r#"serialize(data, { "unsafe": true })"#).len(), 1);
    }


    #[test]
    fn allows_unsafe_false() {
        assert!(run("serialize(data, { unsafe: false })").is_empty());
    }


    #[test]
    fn allows_no_options() {
        assert!(run("serialize(data)").is_empty());
    }


    #[test]
    fn allows_other_options() {
        assert!(run("serialize(data, { isJSON: true })").is_empty());
    }


    #[test]
    fn ignores_non_serialize_call() {
        assert!(run("stringify(data, { unsafe: true })").is_empty());
    }
}
