use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["invariant"])
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

        let is_invariant = match &call.callee {
            oxc_ast::ast::Expression::Identifier(id) => id.name.as_str() == "invariant",
            _ => false,
        };
        if !is_invariant {
            return;
        }

        if call.arguments.len() >= 2 {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`invariant()` without a message — add a descriptive \
                      string as the second argument."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::invariant_requires_message::oxc_typescript::Check;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_invariant_without_message() {
        let diags = run("invariant(router != null);");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_invariant_with_message() {
        assert!(run("invariant(router != null, \"Router must be initialized\");").is_empty());
    }


    #[test]
    fn allows_invariant_with_template_literal() {
        assert!(run("invariant(x > 0, `Expected positive, got ${x}`);").is_empty());
    }


    #[test]
    fn ignores_method_call() {
        assert!(run("const obj = { invariant() {} }; obj.invariant(x);").is_empty());
    }


    #[test]
    fn ignores_other_functions() {
        assert!(run("assert(x > 0);").is_empty());
    }


    #[test]
    fn allows_invariant_with_nested_call() {
        assert!(run("invariant(arr.includes(x), \"missing\");").is_empty());
    }


    #[test]
    fn flags_invariant_with_nested_call_no_message() {
        let diags = run("invariant(arr.includes(x));");
        assert_eq!(diags.len(), 1);
    }
}
