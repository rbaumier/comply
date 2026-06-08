//! no-weak-keys backend — flag weak RSA key lengths and EC curves.

use crate::diagnostic::{Diagnostic, Severity};

/// RSA modulus lengths considered weak (< 2048).
const WEAK_RSA_LENGTHS: &[&str] = &["256", "384", "512", "768", "1024"];

/// EC named curves considered weak (< 256-bit).
const WEAK_CURVES: &[&str] = &["p-128", "p-192", "secp192r1", "secp192k1", "prime192v1"];

/// Extract the inner text of a string node (strip quotes).
fn string_inner<'a>(node: tree_sitter::Node, source: &'a [u8]) -> &'a str {
    let text = node.utf8_text(source).unwrap_or("");
    if text.len() >= 2 {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

/// Check if a property assignment pair has a weak RSA or EC value.
/// Looks for patterns like `modulusLength: 1024` or `namedCurve: 'P-192'`.
fn check_pair(node: tree_sitter::Node, source: &[u8]) -> Option<&'static str> {
    if node.kind() != "pair" {
        return None;
    }
    let key = node.child_by_field_name("key")?;
    let value = node.child_by_field_name("value")?;

    let key_text = key.utf8_text(source).ok()?;

    // Check for weak RSA modulus length.
    if key_text.eq_ignore_ascii_case("modulusLength") {
        let val_text = value.utf8_text(source).unwrap_or("");
        if WEAK_RSA_LENGTHS.contains(&val_text) {
            return Some("Weak RSA key length — use at least 2048 bits.");
        }
    }

    // Check for weak EC curve.
    if (key_text.eq_ignore_ascii_case("namedCurve")
        || key_text.eq_ignore_ascii_case("named_curve")
        || key_text.eq_ignore_ascii_case("curve"))
        && value.kind() == "string"
    {
        let inner = string_inner(value, source).to_ascii_lowercase();
        if WEAK_CURVES.contains(&inner.as_str()) {
            return Some("Weak EC curve — use P-256 or stronger.");
        }
    }

    None
}

crate::ast_check! { prefilter = ["modulusLength", "namedCurve", "named_curve"] => |node, source, ctx, diagnostics|
    if let Some(msg) = check_pair(node, source) {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-weak-keys".into(),
            message: msg.into(),
            severity: Severity::Error,
            span: None,
        });
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_rsa_1024() {
        let d = run_on("const opts = { modulusLength: 1024 };");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("RSA"));
    }

    #[test]
    fn flags_rsa_512() {
        assert_eq!(run_on("const opts = { modulusLength: 512 };").len(), 1);
    }

    #[test]
    fn allows_rsa_2048() {
        assert!(run_on("const opts = { modulusLength: 2048 };").is_empty());
    }

    #[test]
    fn flags_weak_ec_curve_p192() {
        assert_eq!(run_on("const opts = { namedCurve: 'P-192' };").len(), 1);
    }

    #[test]
    fn allows_p256() {
        assert!(run_on("const opts = { namedCurve: 'P-256' };").is_empty());
    }
}
