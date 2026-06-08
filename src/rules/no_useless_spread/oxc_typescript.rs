use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SpreadElement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SpreadElement(spread) = node.kind() else {
            return;
        };

        let parent = semantic.nodes().parent_node(node.id());

        let is_useless = match parent.kind() {
            // `{...{a:1}}` — object spread of an object literal inside object
            AstKind::ObjectExpression(_) => {
                matches!(spread.argument, Expression::ObjectExpression(_))
            }
            // `[...[1,2]]` — array spread of an array literal inside array
            AstKind::ArrayExpression(_) => {
                matches!(spread.argument, Expression::ArrayExpression(_))
            }
            // `fn(...[1,2])` — array spread of an array literal inside arguments
            AstKind::CallExpression(_) | AstKind::NewExpression(_) => {
                matches!(spread.argument, Expression::ArrayExpression(_))
            }
            _ => false,
        };

        if !is_useless {
            return;
        }

        let label = if matches!(spread.argument, Expression::ArrayExpression(_)) {
            "array"
        } else {
            "object"
        };
        let container = match parent.kind() {
            AstKind::ObjectExpression(_) => "object literal",
            AstKind::ArrayExpression(_) => "array literal",
            AstKind::CallExpression(_) | AstKind::NewExpression(_) => "arguments",
            _ => "expression",
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, spread.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Spreading an {label} literal in {container} is unnecessary."),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_array_spread_in_array() {
        assert_eq!(run_on("const x = [...[1, 2, 3]];").len(), 1);
    }


    #[test]
    fn allows_spread_variable_in_array() {
        assert!(run_on("const x = [...arr];").is_empty());
    }


    #[test]
    fn flags_object_spread_in_object() {
        assert_eq!(run_on("const x = {...{a: 1}};").len(), 1);
    }


    #[test]
    fn allows_spread_variable_in_object() {
        assert!(run_on("const x = {...obj};").is_empty());
    }


    #[test]
    fn flags_array_spread_in_call() {
        assert_eq!(run_on("foo(...[1, 2]);").len(), 1);
    }


    #[test]
    fn allows_spread_variable_in_call() {
        assert!(run_on("foo(...args);").is_empty());
    }


    #[test]
    fn allows_array_spread_in_object() {
        // This is a type error, not our concern
        assert!(run_on("const x = {...[1, 2]};").is_empty());
    }


    #[test]
    fn allows_object_spread_in_array() {
        // This is a type error, not our concern
        assert!(run_on("const x = [...{a: 1}];").is_empty());
    }
}
