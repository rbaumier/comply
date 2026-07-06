//! no-weak-ssl backend for Rust.
//!
//! Flags weak SSL/TLS protocol versions (SSLv2, SSLv3, TLSv1.0, TLSv1.1)
//! in string literals and identifiers.

use crate::diagnostic::{Diagnostic, Severity};

const WEAK_PROTOCOLS: &[&str] = &["SSLv2", "SSLv3", "TLSv1.0", "TLSv1.1", "TLSv1"];

fn is_weak_protocol(inner: &str) -> bool {
    for &proto in WEAK_PROTOCOLS {
        if inner.eq_ignore_ascii_case(proto) {
            // "TLSv1" must NOT match "TLSv1.2" or "TLSv1.3".
            if proto == "TLSv1" && inner.len() > 5 {
                continue;
            }
            return true;
        }
    }
    false
}

crate::ast_check! { on ["string_literal", "raw_string_literal"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");
    // Strip surrounding quotes
    let inner = if text.len() >= 2 { &text[1..text.len() - 1] } else { text };

    if !is_weak_protocol(inner) {
        return;
    }

    // A weak-protocol string in match-arm PATTERN position (`match s { "tlsv1" =>
    // … }`, `"tlsv1" | "sslv3" => …`) is a parser recognising a version name, not
    // a value handed to a TLS stack. The direct parent being a `match_pattern`/
    // `or_pattern` distinguishes the pattern side from the arm's value expression
    // (`_ => "tlsv1"`), which is genuine configuration and stays flagged.
    if node
        .parent()
        .is_some_and(|parent| matches!(parent.kind(), "match_pattern" | "or_pattern"))
        && crate::rules::rust_helpers::match_arm_of_pattern(node).is_some()
    {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-weak-ssl".into(),
        message: "Weak SSL/TLS protocol detected — use TLSv1.2 or TLSv1.3.".into(),
        severity: Severity::Error,
        span: None,
    });
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_sslv3() {
        assert_eq!(run_on(r#"fn f() { let proto = "SSLv3"; }"#).len(), 1);
    }

    #[test]
    fn flags_tls10() {
        assert_eq!(run_on(r#"fn f() { let proto = "TLSv1.0"; }"#).len(), 1);
    }

    #[test]
    fn allows_tls12() {
        assert!(run_on(r#"fn f() { let proto = "TLSv1.2"; }"#).is_empty());
    }

    #[test]
    fn allows_tls13() {
        assert!(run_on(r#"fn f() { let proto = "TLSv1.3"; }"#).is_empty());
    }

    #[test]
    fn allows_weak_protocol_in_match_arm_pattern() {
        let src = r#"
            fn to_ssl_version(s: &str) -> V {
                match s {
                    "tlsv1" => V::Tlsv1,
                    "tlsv1.0" => V::Tlsv10,
                    "tlsv1.1" => V::Tlsv11,
                    _ => V::Default,
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_weak_protocol_in_or_pattern() {
        let src = r#"
            fn f(s: &str) -> V {
                match s {
                    "tlsv1" | "sslv3" => V::Weak,
                    _ => V::Ok,
                }
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_weak_protocol_in_argument_position() {
        assert_eq!(run_on(r#"fn f() { configure_tls("tlsv1"); }"#).len(), 1);
    }

    #[test]
    fn flags_weak_protocol_in_match_arm_value() {
        let src = r#"
            fn f(x: bool) -> &'static str {
                match x {
                    true => "tlsv1",
                    false => "TLSv1.2",
                }
            }
        "#;
        assert_eq!(run_on(src).len(), 1);
    }
}
