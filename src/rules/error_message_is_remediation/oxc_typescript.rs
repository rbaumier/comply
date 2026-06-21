//! error-message-is-remediation — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const VERBS: &[&str] = &[
    "is", "are", "was", "were", "be", "been", "has", "have", "had", "do", "does", "did", "will",
    "would", "could", "should", "may", "might", "must", "shall", "can", "need", "check", "verify",
    "ensure", "provide", "specify", "use", "try", "retry", "pass", "set", "add", "remove",
    "update", "create", "delete", "call", "return", "expect", "require", "missing", "failed",
    "cannot", "unable", "exceeded", "denied", "rejected", "not",
];

fn has_verb(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    VERBS
        .iter()
        .any(|v| lower.split_whitespace().any(|w| w == *v))
}

/// Type names that, when named as the expected/required value, make a
/// type-validation message actionable on their own.
const TYPE_NAMES: &[&str] = &[
    "string", "number", "boolean", "bool", "object", "array", "function", "date", "buffer",
    "integer", "int", "float", "bigint", "symbol", "map", "set", "promise", "null", "undefined",
];

fn mentions_type(lower: &str) -> bool {
    TYPE_NAMES
        .iter()
        .any(|t| lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| w == *t))
}

/// A message that points at a specific field via a *delimited* reference, e.g.
/// `Object expected for \`dynamicTemplateData\`` or `"age" must be a number`.
///
/// Requires a balanced delimiter pair (two backticks, two straight quotes, or a
/// curly-quote open/close pair) so a stray in-word apostrophe (`don't`) or a
/// single straight quote does not count as a field reference.
fn references_field(msg: &str) -> bool {
    msg.matches('`').count() >= 2
        || msg.matches('"').count() >= 2
        || msg.matches('\'').count() >= 2
        || (msg.contains('\u{2018}') && msg.contains('\u{2019}')) // ‘ … ’
        || (msg.contains('\u{201c}') && msg.contains('\u{201d}')) // “ … ”
}

/// A type-validation message is already remediation: it names what value to
/// provide for the offending input. Recognises patterns like
/// `Object expected for \`field\``, `expected string`, `must be a number`,
/// `should be an array`, or an `expected … for \`field\`` template that points
/// at the field even when the constraint is an enum (e.g.
/// `desc or asc expected for \`sortByDirection\``).
///
/// A constraint qualifier alone is not enough: it must either name a concrete
/// type or point at the field, so a vague `something expected here` with no
/// type and no field reference stays flagged.
fn is_type_validation_remediation(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    let has_constraint = lower.contains("expected")
        || lower.contains("must be")
        || lower.contains("should be")
        || lower.contains("has to be")
        || lower.contains("needs to be");
    has_constraint && (mentions_type(&lower) || references_field(msg))
}

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Error"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let oxc_ast::AstKind::NewExpression(new_expr) = node.kind() else {
            return;
        };

        if ctx.file.path_segments.in_test_dir {
            return;
        }

        // Check constructor name is "Error".
        let callee_name = match &new_expr.callee.without_parentheses() {
            Expression::Identifier(id) => &*id.name,
            _ => return,
        };
        if callee_name != "Error" {
            return;
        }

        // Get the first argument.
        let Some(first_arg) = new_expr.arguments.first() else {
            return;
        };
        let Some(arg_expr) = first_arg.as_expression() else {
            return;
        };

        // Extract string content.
        let source = semantic.source_text();
        let msg = match arg_expr.without_parentheses() {
            Expression::StringLiteral(s) => &*s.value,
            Expression::TemplateLiteral(t) => {
                // Only handle simple template literals (no expressions).
                if !t.expressions.is_empty() {
                    return;
                }
                if let Some(quasi) = t.quasis.first() {
                    &*quasi.value.raw
                } else {
                    return;
                }
            }
            _ => return,
        };

        let too_short = msg.len() < 15;
        let no_verb = !has_verb(msg) && !is_type_validation_remediation(msg);

        if too_short || no_verb {
            let (line, col) = byte_offset_to_line_col(source, new_expr.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: col,
                rule_id: super::META.id.into(),
                message: format!(
                    "Error message \"{msg}\" is too vague \
                     — describe what went wrong and what to do about it."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_type_expected_for_field() {
        // Issue #5214: `<Type> expected for <field>` encodes both the expected
        // type and the offending field — that is the remediation.
        assert!(run(r#"throw new Error('Object expected for `dynamicTemplateData`');"#).is_empty());
        assert!(run(r#"throw new Error('Array of strings expected for `categories`');"#).is_empty());
        assert!(run(r#"throw new Error('string expected for `sortByMetric`');"#).is_empty());
        assert!(run(r#"throw new Error('number expected for `limit`');"#).is_empty());
        assert!(run(r#"throw new Error('Date expected for `startDate`');"#).is_empty());
    }

    #[test]
    fn allows_enum_expected_for_field() {
        // Enum constraint that points at the field is also remediation.
        assert!(
            run(r#"throw new Error('desc or asc expected for `sortByDirection`');"#).is_empty()
        );
    }

    #[test]
    fn allows_must_be_a_type() {
        assert!(run(r#"throw new Error('"age" must be a number');"#).is_empty());
        assert!(run(r#"throw new Error('value should be an array of items');"#).is_empty());
    }

    #[test]
    fn still_flags_vague_no_type() {
        // No expected type, no remediation, no verb → still vague.
        assert_eq!(run(r#"throw new Error('Invalid input here');"#).len(), 1);
    }

    #[test]
    fn still_flags_expected_without_type() {
        // `expected` qualifier but no concrete type → not actionable.
        assert_eq!(run(r#"throw new Error('something expected here ok');"#).len(), 1);
    }

    #[test]
    fn still_flags_constraint_with_stray_apostrophe() {
        // A lone in-word apostrophe is not a field reference: a constraint
        // qualifier with no type and no balanced delimiter stays flagged.
        assert_eq!(run(r#"throw new Error("o'clock value expected");"#).len(), 1);
    }
}
