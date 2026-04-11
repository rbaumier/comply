//! no-unverified-certificate backend for Rust.
//!
//! Flags `danger_accept_invalid_certs(true)` (reqwest) and
//! `set_verify(SslVerifyMode::NONE)` (openssl) — disabling TLS
//! certificate verification enables MITM attacks.

use crate::diagnostic::{Diagnostic, Severity};

fn has_disabled_cert_verification(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();

    // reqwest: `.danger_accept_invalid_certs(true)`
    if lower.contains("danger_accept_invalid_certs") && lower.contains("true") {
        return true;
    }

    // openssl: `set_verify(SslVerifyMode::NONE)`
    if lower.contains("sslverifymode::none") || lower.contains("ssl_verify_none") {
        return true;
    }

    // rustls: `dangerous().set_certificate_verifier`
    if lower.contains("dangerous()") && lower.contains("certificate_verifier") {
        return true;
    }

    false
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "source_file" {
        return;
    }

    let text = std::str::from_utf8(source).unwrap_or("");
    for (idx, line) in text.lines().enumerate() {
        if has_disabled_cert_verification(line) {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: 1,
                rule_id: "no-unverified-certificate".into(),
                message: "Disabled SSL certificate verification — enables MITM attacks.".into(),
                severity: Severity::Error,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
    }

    #[test]
    fn flags_danger_accept_invalid_certs() {
        assert_eq!(
            run_on("fn f() { client.danger_accept_invalid_certs(true); }").len(),
            1,
        );
    }

    #[test]
    fn flags_ssl_verify_mode_none() {
        assert_eq!(
            run_on("fn f() { ctx.set_verify(SslVerifyMode::NONE); }").len(),
            1,
        );
    }

    #[test]
    fn allows_normal_client() {
        assert!(run_on("fn f() { let client = Client::new(); }").is_empty());
    }
}
