use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const COLLECTIONS: &[&str] = &["Set", "Map", "WeakSet", "WeakMap"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Set", "Map", "WeakSet", "WeakMap"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };
        let Expression::Identifier(ctor) = &new_expr.callee else {
            return;
        };
        if !COLLECTIONS.contains(&ctor.name.as_str()) {
            return;
        }
        if new_expr.arguments.len() != 1 {
            return;
        }
        let arg = &new_expr.arguments[0];
        let Some(arg_expr) = arg.as_expression() else {
            return;
        };
        let Some(label) = useless_arg_label(arg_expr) else {
            return;
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("The {label} argument is useless — remove it."),
            severity: super::META.severity,
            span: None,
        });
    }
}

fn useless_arg_label(expr: &Expression) -> Option<&'static str> {
    match expr {
        Expression::ArrayExpression(arr) => {
            if arr.elements.is_empty() {
                Some("empty array")
            } else {
                None
            }
        }
        Expression::Identifier(id) if id.name == "undefined" => Some("`undefined`"),
        Expression::NullLiteral(_) => Some("`null`"),
        Expression::StringLiteral(s) => {
            if s.value.is_empty() {
                Some("empty string")
            } else {
                None
            }
        }
        Expression::TemplateLiteral(tpl) => {
            if tpl.expressions.is_empty() && tpl.quasis.len() == 1 {
                let raw = &tpl.quasis[0].value.raw;
                if raw.is_empty() {
                    return Some("empty string");
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_new_set_empty_array() {
        let d = run_on("const s = new Set([]);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty array"));
    }

    #[test]
    fn flags_new_map_undefined() {
        let d = run_on("const m = new Map(undefined);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`undefined`"));
    }

    #[test]
    fn flags_new_weakset_null() {
        let d = run_on("const ws = new WeakSet(null);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`null`"));
    }

    #[test]
    fn flags_new_set_empty_string() {
        let d = run_on("const s = new Set(\"\");");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty string"));
    }

    #[test]
    fn allows_new_set_with_values() {
        assert!(run_on("const s = new Set([1, 2, 3]);").is_empty());
    }

    #[test]
    fn allows_new_set_no_args() {
        assert!(run_on("const s = new Set();").is_empty());
    }

    #[test]
    fn allows_new_map_with_entries() {
        assert!(run_on("const m = new Map([[\"a\", 1]]);").is_empty());
    }
}
