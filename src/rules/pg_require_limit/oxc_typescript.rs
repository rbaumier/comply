//! pg-require-limit OXC backend.
//!
//! Flags SQL `SELECT` queries without `LIMIT` in string/template literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use crate::rules::sql_helpers::{contains_word, is_sql_string};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (text, span_start, span_len) = match node.kind() {
            AstKind::StringLiteral(lit) => {
                (lit.value.as_str().to_string(), lit.span.start, (lit.span.end - lit.span.start) as usize)
            }
            AstKind::TemplateLiteral(tpl) => {
                // Concatenate quasis, replacing expressions with spaces
                let mut out = String::new();
                for (i, quasi) in tpl.quasis.iter().enumerate() {
                    out.push_str(quasi.value.raw.as_str());
                    if i < tpl.quasis.len() - 1 {
                        out.push(' ');
                    }
                }
                (out, tpl.span.start, (tpl.span.end - tpl.span.start) as usize)
            }
            _ => return,
        };

        if text.is_empty() {
            return;
        }
        if !is_sql_string(&text) {
            return;
        }
        if !starts_with_select(&text) {
            return;
        }
        let lower = text.to_ascii_lowercase();
        if contains_word(&lower, "limit") {
            return;
        }
        if is_implicitly_bounded(&lower) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, span_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "pg-require-limit".into(),
            message: "SQL `SELECT` without `LIMIT` can return an unbounded number of rows — \
                      add `LIMIT n` or a unique-row predicate (`WHERE id = ...`, `COUNT(..)`)."
                .into(),
            severity: Severity::Error,
            span: Some((span_start as usize, span_len)),
        });
    }
}

fn starts_with_select(text: &str) -> bool {
    let trimmed = text.trim_start();
    let head: String = trimmed
        .chars()
        .take(8)
        .collect::<String>()
        .to_ascii_lowercase();
    head.starts_with("select") || head.starts_with("with ") || head.starts_with("with\t")
}

fn is_implicitly_bounded(lower: &str) -> bool {
    let has_group_by = contains_phrase(lower, "group by");
    if !has_group_by {
        for agg in ["count(", "sum(", "avg(", "min(", "max("] {
            if lower.contains(agg) {
                return true;
            }
        }
    }

    if lower.contains("exists(") || lower.contains("exists (") {
        return true;
    }

    if contains_word(lower, "where") && has_id_equality(lower) {
        return true;
    }

    false
}

fn contains_phrase(lower: &str, phrase: &str) -> bool {
    lower
        .split_whitespace()
        .collect::<Vec<_>>()
        .windows(phrase.split_whitespace().count())
        .any(|window| window.join(" ") == phrase)
}

fn has_id_equality(lower: &str) -> bool {
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'i'
            && i + 1 < bytes.len()
            && bytes[i + 1] == b'd'
            && (i + 2 == bytes.len() || !is_ident_byte(bytes[i + 2]))
            && (i == 0 || !is_ident_byte(bytes[i - 1]) || bytes[i - 1] == b'.')
        {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() {
                if bytes[j] == b'=' {
                    return true;
                }
                if j + 1 < bytes.len()
                    && bytes[j] == b'i'
                    && bytes[j + 1] == b'n'
                    && (j + 2 == bytes.len() || !is_ident_byte(bytes[j + 2]))
                {
                    let mut k = j + 2;
                    while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    if k < bytes.len() && bytes[k] == b'(' {
                        return true;
                    }
                }
            }
        }
        i += 1;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
