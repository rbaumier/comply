//! regex-no-useless-flag OxcCheck backend — visits RegExpLiteral nodes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn has_useless_flag(pattern: &str, flags: &str) -> bool {
    let pbytes = pattern.as_bytes();

    // `i` flag with no unescaped letters outside character classes.
    if flags.contains('i') {
        let mut has_letter = false;
        let mut k = 0;
        while k < pbytes.len() {
            if pbytes[k] == b'\\' {
                k += 2;
                continue;
            }
            if pbytes[k] == b'[' {
                k += 1;
                while k < pbytes.len() && pbytes[k] != b']' {
                    if pbytes[k] == b'\\' {
                        k += 1;
                    }
                    k += 1;
                }
            }
            if k < pbytes.len() && pbytes[k].is_ascii_alphabetic() {
                has_letter = true;
                break;
            }
            k += 1;
        }
        if !has_letter {
            return true;
        }
    }

    // `m` flag with no ^ or $
    if flags.contains('m') {
        let has_anchor = pbytes.contains(&b'^') || pbytes.contains(&b'$');
        if !has_anchor {
            return true;
        }
    }

    // `s` flag with no unescaped `.`
    if flags.contains('s') {
        let mut k = 0;
        let mut has_dot = false;
        while k < pbytes.len() {
            if pbytes[k] == b'\\' {
                k += 2;
                continue;
            }
            if pbytes[k] == b'.' {
                has_dot = true;
                break;
            }
            k += 1;
        }
        if !has_dot {
            return true;
        }
    }

    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else {
            return;
        };
        let pattern = re.regex.pattern.text.as_str();
        let flags_str = re.regex.flags.to_string();
        if flags_str.is_empty() {
            return;
        }
        if !has_useless_flag(pattern, &flags_str) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, re.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Regex flag has no effect on this pattern \u{2014} remove it.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}
