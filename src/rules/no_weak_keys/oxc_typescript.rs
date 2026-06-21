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
    use crate::rules::test_helpers::run_rule_gated;

    // Regression for rbaumier/comply#5118 — panva/oauth4webapi's
    // `tap/modulus_length.ts` generates a weak (1024-bit) RSA key as deliberate
    // input, asserting the library rejects it. The key is never used for a real
    // crypto operation and never ships to production, so the weak-key harm does
    // not apply. The central `skip_in_test_dir` gate suppresses the rule for the
    // node-tap test directory (the issue's exact path, no `.test.` infix).
    #[test]
    fn gated_no_fp_on_weak_key_in_node_tap_test() {
        let src = "const kp = await lib.generateKeyPair('RS256', { modulusLength: 1024 })\n";
        assert!(
            run_rule_gated(&Check, src, "tap/modulus_length.ts").is_empty(),
            "skip_in_test_dir must suppress weak keys in the node-tap test dir"
        );
    }

    // A weak key in a production/source file is a real credential and must keep
    // firing.
    #[test]
    fn gated_still_flags_weak_key_in_production() {
        let src = "const kp = generateKeyPairSync('rsa', { modulusLength: 1024 })\n";
        assert_eq!(
            run_rule_gated(&Check, src, "src/crypto.ts").len(),
            1,
            "production weak key must still be flagged"
        );
    }

    // A weak EC curve in production is equally a real credential.
    #[test]
    fn gated_still_flags_weak_curve_in_production() {
        let src = "const kp = generateKeyPairSync('ec', { namedCurve: 'p-192' })\n";
        assert_eq!(
            run_rule_gated(&Check, src, "src/crypto.ts").len(),
            1,
            "production weak EC curve must still be flagged"
        );
    }
}
