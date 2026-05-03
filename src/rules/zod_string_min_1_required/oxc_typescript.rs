//! zod-string-min-1-required OXC backend — flag `z.string()` calls that
//! are not chained with a length/format/optionality constraint.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const VALID_CONTINUATIONS: &[&str] = &[
    "min",
    "max",
    "email",
    "url",
    "uuid",
    "regex",
    "length",
    "startsWith",
    "endsWith",
    "optional",
    "nullable",
    "nullish",
    "trim",
    "toLowerCase",
    "toUpperCase",
];

fn callee_name(expr: &Expression) -> Option<String> {
    match expr {
        Expression::StaticMemberExpression(m) => {
            let obj = callee_name(&m.object)?;
            Some(format!("{}.{}", obj, m.property.name))
        }
        Expression::Identifier(id) => Some(id.name.to_string()),
        _ => None,
    }
}

/// Check if this call expression is the object of a parent member expression
/// with a valid continuation method. We do this by checking if this call is
/// wrapped in a `z.string().min(1)` style chain via the source text around
/// the call span.
fn is_chained_with_valid_continuation(call_end: u32, source: &str) -> bool {
    let rest = &source[call_end as usize..];
    let trimmed = rest.trim_start();
    if let Some(after_dot) = trimmed.strip_prefix('.') {
        let method: String = after_dot
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        return VALID_CONTINUATIONS.contains(&method.as_str());
    }
    false
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["z.string"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Some(name) = callee_name(&call.callee) else { return };
        if name != "z.string" {
            return;
        }

        // Check if this z.string() is chained with a valid continuation.
        if is_chained_with_valid_continuation(call.span.end, ctx.source) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Bare `z.string()` accepts empty strings \u{2014} add `.min(1)` or a format constraint.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_bare_string() {
        assert_eq!(run("const s = z.object({ name: z.string() })").len(), 1);
    }

    #[test]
    fn allows_min() {
        assert!(run("z.string().min(1)").is_empty());
    }

    #[test]
    fn allows_email() {
        assert!(run("z.string().email()").is_empty());
    }

    #[test]
    fn allows_optional() {
        assert!(run("z.string().optional()").is_empty());
    }
}
