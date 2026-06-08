//! better-result-try-requires-catch OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, ObjectPropertyKind};
use oxc_span::GetSpan;
use std::sync::Arc;

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
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let callee_text = &ctx.source[call.callee.span().start as usize..call.callee.span().end as usize];
        if callee_text != "Result.try" && callee_text != "Result.tryPromise" {
            return;
        }

        // Find first object expression argument
        let obj = call.arguments.iter().find_map(|arg| match arg {
            Argument::ObjectExpression(obj) => Some(obj.as_ref()),
            _ => None,
        });

        let Some(obj) = obj else {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("{callee_text} must receive an object with `try` and `catch`."),
                severity: Severity::Warning,
                span: None,
            });
            return;
        };

        let mut has_try = false;
        let mut has_catch = false;
        for prop in &obj.properties {
            let key_name = match prop {
                ObjectPropertyKind::ObjectProperty(p) => {
                    if p.shorthand {
                        // shorthand property: name === value
                        let start = p.span.start as usize;
                        let end = p.span.end as usize;
                        &ctx.source[start..end.min(ctx.source.len())]
                    } else {
                        match &p.key {
                            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
                            oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value.as_str(),
                            _ => continue,
                        }
                    }
                }
                ObjectPropertyKind::SpreadProperty(_) => continue,
            };
            match key_name {
                "try" => has_try = true,
                "catch" => has_catch = true,
                _ => {}
            }
        }

        if !has_try || !has_catch {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!("{callee_text} must include both `try` and `catch` keys."),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }

    #[test]
    fn flags_missing_catch() {
        let src = "const r = Result.try({ try: () => foo() });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_both_keys() {
        let src = "const r = Result.try({ try: () => foo(), catch: (e) => new E() });";
        assert!(run(src).is_empty());
    }
}
