//! no-incorrect-string-concat OXC backend — flag `"..." + identifier`
//! where the identifier's name suggests it holds a number.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

const NUMERIC_HINTS: &[&str] = &[
    "count", "num", "total", "index", "length", "size", "amount", "qty", "sum", "age", "port",
    "offset", "width", "height", "price", "cost",
];

fn looks_numeric(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    NUMERIC_HINTS.iter().any(|h| lower.contains(h))
}

fn final_ident_name<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        _ => None,
    }
}

fn string_literal_value<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::StringLiteral(lit) => Some(lit.value.as_str()),
        _ => None,
    }
}

/// Longest trimmed length still treated as an ordinal/unit affix rather than a
/// descriptive label (`"ème"`, `"thứ"` are 3 chars; `"items"`, `"Total:"` exceed it).
const MAX_AFFIX_LEN: usize = 4;

/// True if `literal` is a short ordinal/unit token meant to run directly against
/// a number (`"."`, `"th"`, `"º"`, `"er"`, `"#"`, `"thứ "`), as opposed to a
/// descriptive label (`"Total: "`, `" items"`). Affixes are short, single-token,
/// and free of label punctuation; labels are longer or carry word separators.
fn is_ordinal_affix(literal: &str) -> bool {
    let trimmed = literal.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= MAX_AFFIX_LEN
        && !trimmed.chars().any(|c| c.is_whitespace() || c == ':')
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::BinaryExpression(bin) = node.kind() else { return };
        if bin.operator != oxc_ast::ast::BinaryOperator::Addition {
            return;
        }

        let pair = string_literal_value(&bin.left)
            .map(|lit| (lit, &bin.right))
            .or_else(|| string_literal_value(&bin.right).map(|lit| (lit, &bin.left)));

        let flagged = pair.is_some_and(|(literal, ident_side)| {
            final_ident_name(ident_side).is_some_and(looks_numeric) && !is_ordinal_affix(literal)
        });

        if flagged {
            let (line, column) = byte_offset_to_line_col(ctx.source, bin.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Suspicious string concatenation with a numeric variable \u{2014} use explicit conversion or template literals.".into(),
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
    ) -> Vec<Diagnostic> {
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
    fn flags_string_plus_count() {
        assert_eq!(run_on(r#"const msg = "Total: " + itemCount;"#).len(), 1);
    }

    #[test]
    fn flags_string_plus_total() {
        assert_eq!(run_on(r#"console.log("Sum is " + totalAmount);"#).len(), 1);
    }

    #[test]
    fn allows_string_plus_string_var() {
        assert!(run_on(r#"const msg = "Hello " + userName;"#).is_empty());
    }

    #[test]
    fn allows_template_literal() {
        assert!(run_on(r#"const msg = `Total: ${itemCount}`;"#).is_empty());
    }

    // Ordinal formatting: a number joined to a short suffix/prefix affix is the
    // intended output (e.g. date-fns locale ordinalNumber), not a bug. See #1912.
    #[test]
    fn allows_number_plus_ordinal_suffix() {
        assert!(run_on(r#"const number = Number(dirtyNumber); return number + ".";"#).is_empty());
    }

    #[test]
    fn allows_ordinal_prefix_plus_number() {
        assert!(run_on(r#"return "thứ " + number;"#).is_empty());
    }

    #[test]
    fn allows_number_plus_english_ordinal() {
        assert!(run_on(r#"const out = n + "th";"#).is_empty());
    }

    #[test]
    fn allows_number_plus_unit_suffix() {
        assert!(run_on(r#"const css = width + "px";"#).is_empty());
    }

    // A descriptive label is still flagged even when it ends/starts with a space.
    #[test]
    fn flags_number_plus_label_with_space() {
        assert_eq!(run_on(r#"const msg = itemCount + " items found";"#).len(), 1);
    }
}
