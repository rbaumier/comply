//! regex-no-useless-flag OxcCheck backend — visits RegExpLiteral nodes.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// A letter `/i` can fold: alphabetic with a distinct upper/lowercase form.
/// Covers non-ASCII case pairs (`č`/`Č`, `é`/`É`, `ñ`/`Ñ`) and excludes
/// caseless alphabetics (CJK, etc.) where `/i` truly has no effect.
fn is_case_variable_letter(c: char) -> bool {
    c.is_alphabetic() && (c.to_lowercase().next() != Some(c) || c.to_uppercase().next() != Some(c))
}

/// `/i` matters iff the pattern holds a letter with a case to fold. Letters
/// inside a character class (`[a-z]`, `[cč]`) are case-sensitive too, so they
/// count. Iterates by `char` so multi-byte UTF-8 letters are seen as letters,
/// not as ASCII-failing continuation bytes.
fn pattern_has_case_variable_letter(pattern: &str) -> bool {
    let chars: Vec<char> = pattern.chars().collect();
    let mut k = 0;
    while k < chars.len() {
        match chars[k] {
            '\\' => {
                k += 2; // escape sequence (`\d`, `\w`, …) — never a literal letter
            }
            '[' => {
                // Scan the class body: any letter inside is case-sensitive.
                k += 1;
                while k < chars.len() && chars[k] != ']' {
                    if chars[k] == '\\' {
                        k += 1;
                    } else if is_case_variable_letter(chars[k]) {
                        return true;
                    }
                    k += 1;
                }
                k += 1; // past `]`
            }
            c if is_case_variable_letter(c) => return true,
            _ => k += 1,
        }
    }
    false
}

fn has_useless_flag(pattern: &str, flags: &str) -> bool {
    let pbytes = pattern.as_bytes();

    // `i` is useless only when the pattern has no foldable letter.
    if flags.contains('i') && !pattern_has_case_variable_letter(pattern) {
        return true;
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
