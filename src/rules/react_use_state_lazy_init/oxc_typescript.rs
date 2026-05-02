//! react-use-state-lazy-init — oxc backend for TSX.
//!
//! Flags `useState(expensive())` and `useState(window.innerWidth)` where
//! the argument is a call expression or member expression.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
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
        // Check that the callee is `useState`.
        let is_use_state = match &call.callee {
            Expression::Identifier(id) => id.name == "useState",
            _ => false,
        };
        if !is_use_state {
            return;
        }
        // Check first argument is a call expression or member expression.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let expr = first_arg.as_expression();
        let Some(expr) = expr else { return };
        let is_expensive = matches!(
            expr.without_parentheses(),
            Expression::CallExpression(_) | Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) | Expression::PrivateFieldExpression(_)
        );
        if !is_expensive {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "react-use-state-lazy-init".into(),
            message: "`useState(expensive())` runs the initializer on every render \
                      and crashes in SSR. Wrap in a lazy function: \
                      `useState(() => expensive())`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(source, &Check)
    }

    #[test]
    fn flags_use_state_with_function_call() {
        assert_eq!(run_on("const [w] = useState(getInitial());").len(), 1);
    }

    #[test]
    fn flags_use_state_with_browser_api() {
        assert_eq!(run_on("const [w] = useState(window.innerWidth);").len(), 1);
    }

    #[test]
    fn allows_lazy_init() {
        assert!(run_on("const [w] = useState(() => getInitial());").is_empty());
    }

    #[test]
    fn allows_primitive_init() {
        assert!(run_on("const [w] = useState(0);").is_empty());
    }
}
