use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::BinaryExpression, AstType::SwitchStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::BinaryExpression(bin) => {
                let op = bin.operator.as_str();
                if op != "==" && op != "===" {
                    return;
                }
                let (call, literal) = match (&bin.left, &bin.right) {
                    (Expression::CallExpression(call), other)
                    | (other, Expression::CallExpression(call)) => (call.as_ref(), other),
                    _ => return,
                };
                if is_case_mismatch(call, literal) {
                    push_at(diagnostics, ctx, bin.span.start);
                }
            }
            AstKind::SwitchStatement(switch) => {
                let Expression::CallExpression(call) = &switch.discriminant else {
                    return;
                };
                let call = call.as_ref();
                for case in &switch.cases {
                    let Some(test) = &case.test else { continue };
                    if is_case_mismatch(call, test) {
                        push_at(diagnostics, ctx, test.span().start);
                    }
                }
            }
            _ => {}
        }
    }
}

fn push_at(diagnostics: &mut Vec<Diagnostic>, ctx: &CheckCtx, offset: u32) {
    let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
    diagnostics.push(Diagnostic {
        path: Arc::clone(&ctx.path_arc),
        line,
        column,
        rule_id: super::META.id.into(),
        message: super::META.description.into(),
        severity: super::META.severity,
        span: None,
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StringCase {
    Upper,
    Lower,
}

/// True when `call` is a zero-argument `expr.toLowerCase()` / `expr.toUpperCase()`
/// and the value of `literal` contains a cased character contradicting the
/// expected case (so the equality can never hold).
fn is_case_mismatch(call: &oxc_ast::ast::CallExpression, literal: &Expression) -> bool {
    let Some(expected) = expected_case(call) else {
        return false;
    };
    let Some(value) = string_value(literal) else {
        return false;
    };
    value
        .chars()
        .filter_map(char_case)
        .any(|case| case != expected)
}

/// The case of a single character, or `None` for case-less characters
/// (digits, punctuation, control characters, …).
fn char_case(c: char) -> Option<StringCase> {
    if c.is_uppercase() {
        Some(StringCase::Upper)
    } else if c.is_lowercase() {
        Some(StringCase::Lower)
    } else {
        None
    }
}

/// The case a zero-argument `toLowerCase`/`toUpperCase` member call normalises
/// to. `None` for any other callee, or when the call passes arguments.
fn expected_case(call: &oxc_ast::ast::CallExpression) -> Option<StringCase> {
    if !call.arguments.is_empty() {
        return None;
    }
    match member_name(&call.callee)? {
        "toLowerCase" => Some(StringCase::Lower),
        "toUpperCase" => Some(StringCase::Upper),
        _ => None,
    }
}

/// Property name of a member-access callee, whether written with static
/// (`s.toLowerCase`) or computed (`s["toLowerCase"]`, `` s[`toLowerCase`] ``)
/// access. `None` for non-member callees or computed access by a dynamic key.
fn member_name<'a>(callee: &Expression<'a>) -> Option<&'a str> {
    match callee {
        Expression::StaticMemberExpression(member) => Some(member.property.name.as_str()),
        Expression::ComputedMemberExpression(member) => string_value(&member.expression),
        _ => None,
    }
}

/// The static string a string-bearing expression resolves to: a string literal
/// or a no-substitution template literal (`"x"`, `'x'`, `` `x` ``). The value is
/// the decoded (cooked) string, with escape sequences resolved. `None` for
/// templates with substitutions or any other expression.
fn string_value<'a>(expr: &Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::StringLiteral(lit) => Some(lit.value.as_str()),
        Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() => {
            tpl.quasis.first()?.value.cooked.as_ref().map(|s| s.as_str())
        }
        _ => None,
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

    fn count(source: &str) -> usize {
        crate::rules::test_helpers::run_rule(&Check, source, "t.js").len()
    }

    // ---- invalid.js fixtures: each must fire ----

    #[test]
    fn upper_case_lower_literal() {
        assert_eq!(count("s.toUpperCase() === 'abc';"), 1);
        assert_eq!(count("s.toUpperCase() == 'abc';"), 1);
        assert_eq!(count("'abc' === s.toUpperCase();"), 1);
    }

    #[test]
    fn lower_case_escaped_then_upper() {
        assert_eq!(count(r#"s.toLowerCase() === "\u001aX";"#), 1);
        assert_eq!(count(r#"s.toLowerCase() === "\u{001a}X";"#), 1);
        assert_eq!(count(r#"s.toLowerCase() === "\xaaX";"#), 1);
        assert_eq!(count(r#"s.toLowerCase() === "\nX";"#), 1);
    }

    #[test]
    fn inside_conditions() {
        assert_eq!(
            count("if (s.toUpperCase() === 'abc' && c == d && e == f) {};"),
            1
        );
        assert_eq!(
            count("while (s.toUpperCase() === 'abc' && c == d && e == f) {};"),
            1
        );
        assert_eq!(count("while (s.toUpperCase() === 'abc') {};"), 1);
        assert_eq!(count("do {} while (s.toLowerCase() === 'ABC');"), 1);
        assert_eq!(count("for (; s.toLowerCase() === 'ABC'; ) {};"), 1);
    }

    #[test]
    fn no_substitution_template_literal() {
        assert_eq!(count("let b = s.toLowerCase() === `eFg`;"), 1);
    }

    #[test]
    fn switch_discriminant() {
        // 'abc' and 'aBc' mismatch upper case; 'ABC' matches.
        assert_eq!(
            count("switch (s.toUpperCase()) { case 'ABC': case 'abc': case 'aBc': default: }"),
            2
        );
    }

    #[test]
    fn computed_member_access() {
        assert_eq!(count("for (; s['toLowerCase']() === 'ABC'; ) {}"), 1);
        assert_eq!(count("for (; s[`toUpperCase`]() === 'abc'; ) {}"), 1);
    }

    #[test]
    fn computed_member_access_switch() {
        // 'Abc', 'aBc', 'abC' all mismatch lower case.
        assert_eq!(
            count("switch (s['toLowerCase']()) { case 'Abc': case 'aBc': case 'abC': default: }"),
            3
        );
    }

    // ---- valid.js fixtures: none must fire ----

    #[test]
    fn matching_case_literals() {
        assert_eq!(count("s.toUpperCase() === 'ABC';"), 0);
        assert_eq!(count("s.toLowerCase() === 'abc';"), 0);
        assert_eq!(count("s.toUpperCase() === '20';"), 0);
        assert_eq!(count("s.toLowerCase() === '20';"), 0);
    }

    #[test]
    fn template_with_substitution_is_dynamic() {
        assert_eq!(count("s.toLowerCase() === `eFg${12}`;"), 0);
        assert_eq!(count("s.toLowerCase() == `eFg${12}`;"), 0);
    }

    #[test]
    fn escape_only_payloads_have_no_case() {
        assert_eq!(count(r#"s.toLowerCase() === "\xaa";"#), 0);
        assert_eq!(count(r#"s.toLowerCase() === "\xAA";"#), 0);
        assert_eq!(count(r#"s.toUpperCase() === "\u001b";"#), 0);
        assert_eq!(count(r#"s.toLowerCase() === "\u001B";"#), 0);
        assert_eq!(count(r#"s.toUpperCase() === "\u000D";"#), 0);
        assert_eq!(count(r#"s.toLowerCase() === "\u000D";"#), 0);
    }

    #[test]
    fn brace_escape_then_matching_case() {
        assert_eq!(count(r#"s.toLowerCase() === "\u{a}aa";"#), 0);
        assert_eq!(count(r#"s.toLowerCase() === "\u{A}aa";"#), 0);
    }

    #[test]
    fn non_letters_only() {
        assert_eq!(count(r#"s.toUpperCase() === "{}";"#), 0);
        assert_eq!(count(r#"s.toLowerCase() === "{}";"#), 0);
    }

    // ---- scope guards beyond the Biome fixtures ----

    #[test]
    fn ignores_inequality_operators() {
        // Biome only handles == / === ; != / !== are out of scope.
        assert_eq!(count("s.toUpperCase() !== 'abc';"), 0);
        assert_eq!(count("s.toUpperCase() != 'abc';"), 0);
    }

    #[test]
    fn ignores_locale_variants() {
        assert_eq!(count("s.toLocaleLowerCase() === 'ABC';"), 0);
        assert_eq!(count("s.toLocaleUpperCase() === 'abc';"), 0);
    }

    #[test]
    fn ignores_call_with_arguments() {
        assert_eq!(count("s.toUpperCase('x') === 'abc';"), 0);
    }
}

