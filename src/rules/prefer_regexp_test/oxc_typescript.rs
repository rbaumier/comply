//! prefer-regexp-test OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Check if the parent node represents a boolean context.
fn is_boolean_context(
    node: &oxc_semantic::AstNode,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    match parent.kind() {
        AstKind::IfStatement(_) | AstKind::WhileStatement(_) | AstKind::DoWhileStatement(_) => {
            true
        }
        AstKind::UnaryExpression(unary) => {
            // `!str.match(...)` or `!!str.match(...)`
            matches!(unary.operator, oxc_ast::ast::UnaryOperator::LogicalNot)
        }
        AstKind::LogicalExpression(log) => {
            // `??` selects a value, never a boolean coercion — `match() ?? []`.
            if log.operator == oxc_ast::ast::LogicalOperator::Coalesce {
                return false;
            }
            // `&&`/`||` are a boolean context for an operand only when the
            // logical expression's own result lands in one. `match() || []`
            // destructured/iterated is a value position, not a boolean test.
            is_boolean_context(parent, semantic)
        }
        AstKind::ParenthesizedExpression(_) => {
            // Recurse up: `if ((str.match(...)))`
            is_boolean_context(parent, semantic)
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".match"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be `.match`
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "match" {
            return;
        }

        // First argument must be a regex literal
        let has_regex_arg = call.arguments.first().is_some_and(|arg| {
            matches!(arg, oxc_ast::ast::Argument::RegExpLiteral(_))
        });
        if !has_regex_arg {
            return;
        }

        // Only flag if in a boolean context
        if !is_boolean_context(node, semantic) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `RegExp#test()` over `String#match()` in boolean contexts.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_match_in_if() {
        let d = run_on("if (str.match(/foo/)) {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-regexp-test");
    }

    #[test]
    fn flags_match_with_double_bang() {
        let d = run_on("const ok = !!str.match(/bar/);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_match_outside_boolean() {
        assert!(run_on("const m = str.match(/foo/);").is_empty());
    }

    #[test]
    fn flags_or_in_if_test() {
        // `||` result lands in the `if` test → boolean context.
        let d = run_on("if (a.match(/x/) || b.match(/y/)) {}");
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn flags_and_in_if_test() {
        let d = run_on("if (s.match(/x/) && cond) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_match_or_array_destructured() {
        // #3924: `match() || []` destructured is a value context, not boolean.
        assert!(run_on(r#"const [, p = ""] = s.match(/x/) || [];"#).is_empty());
    }

    #[test]
    fn allows_match_or_array_iterated() {
        // #3924: `match() || []` consumed via `.reduce()` is a value context.
        assert!(run_on("const temps = s.match(/t/g) || []; temps.reduce(f, 0);").is_empty());
    }

    #[test]
    fn allows_match_coalesce_array() {
        // #3851: `??` selects a value, never a boolean coercion.
        assert!(run_on("const words = s.match(/x/g) ?? [];").is_empty());
    }
}
