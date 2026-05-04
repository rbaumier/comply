//! no-weak-keys oxc backend — flag weak RSA key lengths and EC curves.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, PropertyKey};
use std::sync::Arc;

/// RSA modulus lengths considered weak (< 2048).
const WEAK_RSA_LENGTHS: &[&str] = &["256", "384", "512", "768", "1024"];

/// EC named curves considered weak (< 256-bit).
const WEAK_CURVES: &[&str] = &["p-128", "p-192", "secp192r1", "secp192k1", "prime192v1"];

pub struct Check;

fn key_name<'a>(key: &'a PropertyKey<'a>) -> Option<&'a str> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.as_str()),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str()),
        _ => None,
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["modulusLength", "namedCurve", "named_curve"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else { return };

        let Some(key_text) = key_name(&prop.key) else { return };

        // Check for weak RSA modulus length.
        if key_text.eq_ignore_ascii_case("modulusLength") {
            let val_text = match &prop.value {
                Expression::NumericLiteral(n) => {
                    let v = n.value as u64;
                    // Check directly against weak lengths.
                    if matches!(v, 256 | 384 | 512 | 768 | 1024) {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, prop.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Weak RSA key length — use at least 2048 bits.".into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                    return;
                }
                Expression::StringLiteral(s) => s.value.as_str().to_string(),
                _ => return,
            };
            if WEAK_RSA_LENGTHS.contains(&val_text.as_str()) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, prop.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Weak RSA key length — use at least 2048 bits.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            return;
        }

        // Check for weak EC curve.
        if key_text.eq_ignore_ascii_case("namedCurve")
            || key_text.eq_ignore_ascii_case("named_curve")
            || key_text.eq_ignore_ascii_case("curve")
        {
            let Expression::StringLiteral(s) = &prop.value else { return };
            let inner = s.value.as_str().to_ascii_lowercase();
            if WEAK_CURVES.contains(&inner.as_str()) {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, prop.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Weak EC curve — use P-256 or stronger.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
    }
}
