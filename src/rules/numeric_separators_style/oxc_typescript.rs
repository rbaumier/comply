//! numeric-separators-style OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

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
        "0x" => 2,
        "0b" | "0o" => 4,
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
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.file.path_segments.in_test_dir {
            return;
        }
        let AstKind::NumericLiteral(lit) = node.kind() else {
            return;
        };
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
}
