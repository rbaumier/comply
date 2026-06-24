//! numeric-separators-style OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// True when this numeric literal is the key of an object property
/// (e.g. `{ 110000: '...' }`). Such keys are fixed-length opaque
/// identifiers — area codes, postal codes, HTTP status maps — not
/// quantities, so digit-grouping separators would corrupt their identity.
fn is_object_property_key<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let parent = semantic.nodes().parent_node(node.id());
    let AstKind::ObjectProperty(prop) = parent.kind() else {
        return false;
    };
    prop.key.span() == node.kind().span()
}

/// Extract a bare-decimal literal's digit string, returning `None` for any
/// literal that is not a plain base-10 integer (hex/binary/octal prefix,
/// fractional/exponent form, BigInt suffix). Underscores are stripped so an
/// already-separated literal compares by its bare digits.
fn bare_decimal_digits(raw: &str) -> Option<String> {
    if raw.contains('.') || raw.contains('e') || raw.contains('E') || raw.contains('n') {
        return None;
    }
    let digits: String = raw.chars().filter(|&c| c != '_').collect();
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(digits)
}

/// True when this literal is an element of an array whose elements are *all*
/// bare-decimal integers written using only the digits 0 and 1, all of the
/// same digit length — a uniform binary-pattern table (e.g. CODE128 barcode
/// module patterns: `[11011001100, 11001101100, ...]`). Such values are
/// fixed-width bit fields written in decimal notation, not quantities;
/// thousands-grouping separators would corrupt the bit alignment.
///
/// The uniformity requirement (every element all-0/1 *and* the same length)
/// is what keeps this from over-exempting round magnitudes: a lone `1000000`
/// or a mixed array `[1000000, 2500000]` is never all-0/1 across uniform-length
/// elements, so it stays flagged.
fn is_uniform_binary_pattern_element<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &str,
) -> bool {
    use oxc_ast::ast::{ArrayExpressionElement, Expression};

    let parent = semantic.nodes().parent_node(node.id());
    let AstKind::ArrayExpression(array) = parent.kind() else {
        return false;
    };

    let mut width: Option<usize> = None;
    let mut count = 0usize;

    for element in &array.elements {
        let Some(expr) = element.as_expression() else {
            // A spread or elision means this is not a plain literal table.
            return false;
        };
        let Expression::NumericLiteral(lit) = expr else {
            return false;
        };
        let raw = &source[lit.span.start as usize..lit.span.end as usize];
        let Some(digits) = bare_decimal_digits(raw) else {
            return false;
        };
        if !digits.bytes().all(|b| b == b'0' || b == b'1') {
            return false;
        }
        match width {
            None => width = Some(digits.len()),
            Some(w) if w == digits.len() => {}
            Some(_) => return false,
        }
        count += 1;
    }

    // A single-element array is not a recognizable table.
    count >= 2
}

/// Insert underscores every `group` digits from right to left.
fn add_separators(digits: &str, group: usize) -> String {
    let clean: String = digits.chars().filter(|&c| c != '_').collect();
    if clean.len() < group + 1 {
        return clean;
    }
    let mut result = Vec::new();
    for (i, ch) in clean.chars().rev().enumerate() {
        if i > 0 && i % group == 0 {
            result.push('_');
        }
        result.push(ch);
    }
    result.reverse();
    result.into_iter().collect()
}

fn format_prefixed(prefix: &str, digits: &str, suffix: &str) -> String {
    let group = match prefix.to_lowercase().as_str() {
        "0b" | "0o" => 4,
        // Hex grouping is domain-dependent: colors group by bytes
        // (0xFF_AA_BB), addresses by 4 (0xDEAD_BEEF), Unicode codepoints
        // not at all (0x10FFFF). No single grouping is correct, so comply
        // does not enforce separators on hex literals.
        _ => return format!("{}{}{}", prefix, digits, suffix),
    };
    let formatted = add_separators(digits, group);
    format!("{}{}{}", prefix, formatted, suffix)
}

fn format_decimal(raw: &str, suffix: &str) -> String {
    let clean: String = raw.chars().filter(|&c| c != '_').collect();
    if clean.len() < 5 {
        return format!("{}{}", raw, suffix);
    }
    let formatted = add_separators(raw, 3);
    format!("{}{}", formatted, suffix)
}

fn expected_format(raw: &str) -> Option<String> {
    let (body, suffix) = if let Some(stripped) = raw.strip_suffix('n') {
        (stripped, "n")
    } else {
        (raw, "")
    };

    if body.len() < 2 {
        return None;
    }

    if body.starts_with("0x")
        || body.starts_with("0X")
        || body.starts_with("0b")
        || body.starts_with("0B")
        || body.starts_with("0o")
        || body.starts_with("0O")
    {
        let prefix = &body[..2];
        let digits = &body[2..];
        let formatted = format_prefixed(prefix, digits, suffix);
        if formatted != raw {
            return Some(formatted);
        }
        return None;
    }

    if body.contains('.') || body.contains('e') || body.contains('E') {
        return None;
    }

    let formatted = format_decimal(body, suffix);
    if formatted != raw {
        return Some(formatted);
    }

    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[oxc_ast::AstType::NumericLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        let AstKind::NumericLiteral(lit) = node.kind() else {
            return;
        };
        if is_object_property_key(node, semantic) {
            return;
        }
        if is_uniform_binary_pattern_element(node, semantic, ctx.source) {
            return;
        }
        let raw = &ctx.source[lit.span.start as usize..lit.span.end as usize];
        if let Some(formatted) = expected_format(raw) {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, lit.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Invalid group length in numeric value: `{}` should be `{}`.",
                    raw, formatted
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_separated_four_digit_number() {
        assert!(run_on("const x = 1_000;").is_empty());
    }

    #[test]
    fn flags_unseparated_five_digit_number() {
        let d = run_on("const x = 10000;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("10_000"));
    }

    // Regression #1020: hex grouping is domain-dependent (Unicode
    // codepoints, addresses, colors all differ) — comply does not enforce
    // separators on hex literals.
    #[test]
    fn allows_hex_literals_issue_1020() {
        assert!(run_on("const MAX_CODEPOINT = 0x10FFFF;").is_empty());
        assert!(run_on("const addr = 0xDEADBEEF;").is_empty());
        assert!(run_on("const color = 0xFFAABB;").is_empty());
    }

    // Regression #1087: zero-padded hex bitmask/flag and hash constants are
    // intentionally aligned and must not have separators forced on them.
    #[test]
    fn allows_zero_padded_hex_constants_issue_1087() {
        assert!(run_on("const v = 0x0001;").is_empty());
        assert!(run_on("const v = 0x00000001;").is_empty());
        assert!(run_on("const v = 0x80000000;").is_empty());
        assert!(run_on("const hash = 0x0bcaa747;").is_empty());
    }

    // Binary literals keep nibble grouping — that convention is unambiguous.
    #[test]
    fn still_groups_long_binary_literal() {
        let d = run_on("const flags = 0b101010101;");
        assert_eq!(d.len(), 1, "{:?}", d);
    }

    // Regression #4713: numeric object-property keys are fixed-length opaque
    // identifiers (area codes, postal codes, HTTP status maps), not quantities —
    // forcing separators corrupts their identity.
    #[test]
    fn allows_numeric_object_property_keys_issue_4713() {
        assert!(
            run_on("const areaList = { 110000: '北京市', 120000: '天津市', 130000: '河北省' };")
                .is_empty()
        );
        assert!(run_on("const statusText = { 404: 'Not Found', 500000: 'x' };").is_empty());
    }

    // A numeric literal in value position is still a quantity and stays flagged.
    #[test]
    fn still_flags_numeric_property_value_issue_4713() {
        let d = run_on("const cfg = { timeout: 100000 };");
        assert_eq!(d.len(), 1, "{:?}", d);
        assert!(d[0].message.contains("100_000"));
    }

    // When both key and value are numeric, only the value (a quantity) is
    // flagged — the key (an identifier) is exempt.
    #[test]
    fn flags_only_value_when_key_and_value_numeric_issue_4713() {
        let d = run_on("const m = { 100000: 200000 };");
        assert_eq!(d.len(), 1, "{:?}", d);
        assert!(d[0].message.contains("200_000"));
    }

    // Regression #5928: a uniform binary-pattern table (CODE128 barcode module
    // patterns) — an array whose elements are all all-0/1-digit literals of the
    // same width — is a bit-field table written in decimal, not quantities.
    #[test]
    fn allows_uniform_binary_pattern_array_issue_5928() {
        assert!(
            run_on(
                "export const BARS = [11011001100, 11001101100, 11001100110, 10010011000];"
            )
            .is_empty()
        );
    }

    // A lone all-0/1 round magnitude is still a quantity and stays flagged.
    #[test]
    fn still_flags_lone_round_million_issue_5928() {
        let d = run_on("const x = 1000000;");
        assert_eq!(d.len(), 1, "{:?}", d);
        assert!(d[0].message.contains("1_000_000"));
    }

    // A mixed-magnitude array is not a uniform bit table — both elements are
    // quantities and stay flagged.
    #[test]
    fn still_flags_mixed_magnitude_array_issue_5928() {
        let d = run_on("const xs = [1000000, 2500000];");
        assert_eq!(d.len(), 2, "{:?}", d);
    }

    // An all-0/1 round magnitude sitting in an array of non-binary elements is
    // not part of a uniform binary table, so it stays flagged.
    #[test]
    fn still_flags_all_binary_digit_in_non_uniform_array_issue_5928() {
        let d = run_on("const xs = [1000000, 9999999];");
        assert_eq!(d.len(), 2, "{:?}", d);
    }

    // An array of same-width all-0/1 literals where one element differs in
    // width is not uniform — every element stays flagged.
    #[test]
    fn still_flags_non_uniform_width_binary_array_issue_5928() {
        let d = run_on("const xs = [11011001100, 1100110];");
        assert_eq!(d.len(), 2, "{:?}", d);
    }

    // A lone all-0/1 literal outside any array stays flagged — the structural
    // signal requires the uniform-array context.
    #[test]
    fn still_flags_lone_binary_pattern_issue_5928() {
        let d = run_on("const x = 11011001100;");
        assert_eq!(d.len(), 1, "{:?}", d);
    }
}
