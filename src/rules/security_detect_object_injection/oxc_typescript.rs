//! security-detect-object-injection oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ComputedMemberExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ComputedMemberExpression(member) = node.kind() else {
            return;
        };
        // Skip when the key is a literal (string / number / template
        // with no interpolations) — that's a static lookup, not an
        // injection vector.
        match &member.expression {
            Expression::StringLiteral(_) | Expression::NumericLiteral(_) => return,
            Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() => return,
            _ => {}
        }
        // Skip array literal access `arr[0]` and similar — the rule
        // targets OBJECT injection, not array indexing. Heuristic:
        // if the object is itself an array literal, skip.
        if matches!(&member.object, Expression::ArrayExpression(_)) {
            return;
        }
        // Reasonable false-positive class: when the parent is an
        // AssignmentExpression's left side (`obj[key] = …`) the rule
        // STILL applies — assignment is even riskier than read.
        // No skip here.
        let _ = semantic;
        let (line, column) = byte_offset_to_line_col(ctx.source, member.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Bracket access with a non-literal key — vulnerable to prototype \
                      pollution / data exfiltration if the key comes from untrusted \
                      input. Validate the key against an allowlist first."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_dynamic_bracket_access() {
        let src = r#"function f(obj, key) { return obj[key]; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_static_string_key() {
        let src = r#"function f(obj) { return obj["foo"]; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_array_literal_index() {
        let src = r#"const x = ["a", "b", "c"][i];"#;
        assert!(run(src).is_empty());
    }
}
