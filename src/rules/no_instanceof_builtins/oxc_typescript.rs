//! no-instanceof-builtins OXC backend — flag `x instanceof Array` and other builtins.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const BUILTINS: &[&str] = &[
    "Array",
    "ArrayBuffer",
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "RegExp",
    "Promise",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["instanceof"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };
        if bin.operator != oxc_ast::ast::BinaryOperator::Instanceof {
            return;
        }

        let Expression::Identifier(id) = &bin.right else { return };
        let name = id.name.as_str();
        if !BUILTINS.contains(&name) {
            return;
        }

        let suggestion = if name == "Array" {
            "Use `Array.isArray(x)` instead.".to_string()
        } else {
            format!("Avoid `instanceof {name}` — it fails across realms.")
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: suggestion,
            severity: Severity::Warning,
            span: None,
        });
    }
}
