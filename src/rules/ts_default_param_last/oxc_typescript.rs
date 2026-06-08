//! ts-default-param-last OXC backend — flag default/optional parameters
//! that are not at the end of the parameter list.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ArrowFunctionExpression, AstType::Function]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let params = match node.kind() {
            AstKind::ArrowFunctionExpression(arrow) => &arrow.params,
            AstKind::Function(func) => &func.params,
            _ => return,
        };

        // Walk from the end. Once we see a plain required param, all
        // default/optional params before it are violations.
        // Rest params live in `params.rest`, not in `items`.
        let mut seen_plain = false;
        for param in params.items.iter().rev() {
            let is_default = param.initializer.is_some()
                || param.pattern.is_assignment_pattern();
            let is_optional = param.optional;

            if !is_default && !is_optional {
                seen_plain = true;
                continue;
            }

            if seen_plain {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, param.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Default parameters should be last.".into(),
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
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_default_param_before_required() {
        let diags = run_on("function foo(a = 1, b: number) {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Default parameters"));
    }


    #[test]
    fn allows_default_param_last() {
        let diags = run_on("function foo(a: number, b = 1) {}");
        assert!(diags.is_empty());
    }


    #[test]
    fn allows_all_default_params() {
        let diags = run_on("function foo(a = 1, b = 2) {}");
        assert!(diags.is_empty());
    }
}
