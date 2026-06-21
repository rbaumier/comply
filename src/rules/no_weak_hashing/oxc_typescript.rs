//! no-weak-hashing — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

const WEAK_ALGOS: &[&str] = &["md5", "sha1"];

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["md5", "MD5", "sha1", "SHA1"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let oxc_ast::AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let source = semantic.source_text();

        // Skip files that compute a protocol-mandated digest (e.g. the RFC 1864
        // `Content-MD5` header or the RFC 6455 WebSocket accept key): there the
        // algorithm is dictated by the wire format, not chosen for security, so
        // "use SHA-256" would break interop.
        if crate::oxc_helpers::references_protocol_mandated_weak_hash(source) {
            return;
        }

        // Match `createHash('md5')` / `createHash("sha1")` — direct or member call.
        let is_create_hash = match &call.callee {
            Expression::Identifier(id) => &*id.name == "createHash",
            Expression::StaticMemberExpression(mem) => &*mem.property.name == "createHash",
            _ => false,
        };

        if is_create_hash {
            // Check first argument for weak algo.
            if let Some(first_arg) = call.arguments.first()
                && let Some(expr) = first_arg.as_expression()
                    && let Expression::StringLiteral(s) = expr.without_parentheses() {
                        let inner = s.value.to_ascii_lowercase();
                        if WEAK_ALGOS.contains(&inner.as_str()) {
                            let (line, col) =
                                byte_offset_to_line_col(source, call.span().start as usize);
                            diagnostics.push(Diagnostic {
                                path: Arc::clone(&ctx.path_arc),
                                line,
                                column: col,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "Weak hashing algorithm `createHash('{}')` — use SHA-256 or stronger.",
                                    inner,
                                ),
                                severity: Severity::Error,
                                span: None,
                            });
                        }
                    }
            return;
        }

        // Match bare `MD5(...)` / `SHA1(...)` calls.
        let callee_name = match &call.callee {
            Expression::Identifier(id) => &*id.name,
            _ => return,
        };

        if callee_name == "MD5" || callee_name == "SHA1" {
            let (line, col) = byte_offset_to_line_col(source, call.span().start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: col,
                rule_id: super::META.id.into(),
                message: format!(
                    "Weak hashing algorithm `{}` — use SHA-256 or stronger.",
                    callee_name,
                ),
                severity: Severity::Error,
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_rule_gated;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_md5_create_hash() {
        let d = run_on("const h = crypto.createHash('md5');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("md5"));
    }

    #[test]
    fn flags_bare_md5_call() {
        assert_eq!(run_on("const hash = MD5(data);").len(), 1);
    }

    #[test]
    fn allows_sha256() {
        assert!(run_on("const h = crypto.createHash('sha256');").is_empty());
    }

    // RFC 1864: `Content-MD5` mandates MD5, so the digest is a protocol field,
    // not a security choice. Reproduces fastify/fastify reply-trailers.test.js.
    #[test]
    fn allows_md5_for_content_md5_trailer() {
        let src = r#"
            reply.trailer('Content-MD5', function (reply, payload, done) {
              const hash = createHash('md5')
              hash.update(payload)
              done(null, hash.digest('hex'))
            })
        "#;
        assert!(run_on(src).is_empty());
    }

    // RFC 6455: the WebSocket accept key is a SHA-1 of the client key — dictated
    // by the handshake, not chosen for collision resistance.
    #[test]
    fn allows_sha1_for_websocket_accept_key() {
        let src = r#"
            const accept = createHash('sha1')
              .update(req.headers['sec-websocket-key'] + GUID)
              .digest('base64')
        "#;
        assert!(run_on(src).is_empty());
    }

    // A genuine weak-crypto use (password hashing) still fires even when the file
    // is unrelated to any protocol field.
    #[test]
    fn still_flags_md5_password_hash() {
        let src = "const digest = createHash('md5').update(password).digest('hex');";
        assert_eq!(run_on(src).len(), 1);
    }

    // Regression for rbaumier/comply#5198 — nodemailer's `test/dkim/dkim-test.js`
    // hashes encoded output with MD5 to compare against a hardcoded expected
    // digest. That is a non-cryptographic fixture checksum, not a security
    // primitive, and never ships to production. The central `skip_in_test_dir`
    // gate suppresses the rule for the `test/` directory (the issue's exact path,
    // a `-test.js` suffix, no `.test.` infix).
    #[test]
    fn gated_no_fp_on_md5_checksum_in_test_dir() {
        let src = "const digest = crypto.createHash('md5').update(message).digest('hex');";
        assert!(
            run_rule_gated(&Check, src, "test/dkim/dkim-test.js").is_empty(),
            "skip_in_test_dir must suppress a fixture checksum in the test dir"
        );
    }

    // A weak hash in a production/source file is a real security primitive and
    // must keep firing.
    #[test]
    fn gated_still_flags_md5_in_production() {
        let src = "const digest = createHash('md5').update(password).digest('hex');";
        assert_eq!(
            run_rule_gated(&Check, src, "src/crypto.ts").len(),
            1,
            "production weak hash must still be flagged"
        );
    }

    // SHA-1 in production is equally a broken security primitive.
    #[test]
    fn gated_still_flags_sha1_in_production() {
        let src = "const sig = createHash('sha1').update(token).digest('hex');";
        assert_eq!(
            run_rule_gated(&Check, src, "src/crypto.ts").len(),
            1,
            "production weak SHA-1 hash must still be flagged"
        );
    }
}
