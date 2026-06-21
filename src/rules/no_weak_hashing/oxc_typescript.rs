//! no-weak-hashing — OxcCheck backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use oxc_ast::AstKind;
use oxc_ast::ast::{BinaryOperator, Expression, LogicalOperator};
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

        // Skip a digest computed only when an input overflows a length limit
        // (`if (name.length > 128) { name = createHash('sha1')…digest() }`). That
        // is a deterministic name-shortening fingerprint — e.g. fitting an
        // auto-generated index/constraint identifier into a database engine's
        // identifier-length cap — not a security hash: a password, signature,
        // HMAC or token is never gated on the input being too long. The signal is
        // the enclosing length-overflow comparison, not the variable names, so it
        // survives renaming and does not slip genuine crypto uses through.
        if enclosed_in_length_overflow_guard(node.id(), semantic) {
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
                        // Redis content-addresses a server-side Lua script by the
                        // SHA-1 of its body: `SCRIPT LOAD` returns that digest and
                        // `EVALSHA <sha1>` invokes the script by it. The algorithm
                        // is mandated by the Redis protocol — collision resistance
                        // is irrelevant — so a SHA-1 digest in a file that calls a
                        // Redis script-cache API is not a security hash. This is
                        // SHA-1-only: MD5 has no Redis role, so an MD5 digest still
                        // fires even here (passwords/signatures must stay flagged).
                        if inner == "sha1" && file_calls_redis_script_cache(semantic) {
                            return;
                        }
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

/// True when the call node sits inside an `if`/ternary whose condition tests a
/// `.length` overflow (`x.length > N` or `x.length >= N`). Walks ancestors and
/// inspects each conditional's test for a length comparison; logical `&&`/`||`
/// chains and parentheses in the test are traversed so a compound guard
/// (`if (x.length > N && y)`) still matches.
fn enclosed_in_length_overflow_guard(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic<'_>,
) -> bool {
    let nodes = semantic.nodes();
    nodes.ancestors(node_id).any(|ancestor| match ancestor.kind() {
        AstKind::IfStatement(if_stmt) => test_is_length_overflow(&if_stmt.test),
        AstKind::ConditionalExpression(cond) => test_is_length_overflow(&cond.test),
        _ => false,
    })
}

/// True when `expr` is — or contains via `&&`/`||`/parentheses — a comparison of
/// the form `<member>.length > <numeric literal>` or `>= <numeric literal>` (or
/// the flipped `N < x.length` / `N <= x.length`), i.e. an "input too long" test.
fn test_is_length_overflow(expr: &Expression) -> bool {
    match expr.without_parentheses() {
        Expression::LogicalExpression(logical)
            if matches!(logical.operator, LogicalOperator::And | LogicalOperator::Or) =>
        {
            test_is_length_overflow(&logical.left) || test_is_length_overflow(&logical.right)
        }
        Expression::BinaryExpression(bin) => match bin.operator {
            BinaryOperator::GreaterThan | BinaryOperator::GreaterEqualThan => {
                is_length_access(&bin.left) && is_numeric_literal(&bin.right)
            }
            BinaryOperator::LessThan | BinaryOperator::LessEqualThan => {
                is_numeric_literal(&bin.left) && is_length_access(&bin.right)
            }
            _ => false,
        },
        _ => false,
    }
}

/// True when `expr` is a `<something>.length` member access.
fn is_length_access(expr: &Expression) -> bool {
    matches!(
        expr.without_parentheses(),
        Expression::StaticMemberExpression(member) if member.property.name.as_str() == "length"
    )
}

/// True when `expr` is a numeric literal.
fn is_numeric_literal(expr: &Expression) -> bool {
    matches!(expr.without_parentheses(), Expression::NumericLiteral(_))
}

/// Redis script-cache method names. Calling any of these means the file talks to
/// the Redis script cache, which is keyed by the SHA-1 of the Lua script body:
/// `EVALSHA`/`EVALSHA_RO` invoke by digest, `SCRIPT LOAD` registers and returns
/// it, and ioredis's `defineCommand` builds an EVALSHA wrapper. Method names are
/// matched case-insensitively because client libraries vary the casing
/// (node-redis `evalSha`/`scriptLoad`, ioredis `evalsha`/`evalshaRo`).
const REDIS_SCRIPT_CACHE_METHODS: &[&str] =
    &["evalsha", "evalsharo", "scriptload", "definecommand"];

/// Subcommands of the Redis `SCRIPT` command that manage the script cache — used
/// via the generic-command form `client.script('load', ...)` /
/// `client.script('exists', <sha1>, ...)` (node_redis style). Matched
/// case-insensitively against the first string argument of a `.script(...)` call.
const REDIS_SCRIPT_SUBCOMMANDS: &[&str] = &["load", "exists"];

/// True when the file contains a call to a Redis script-cache API. The signal is
/// a genuine method call anchored to a Redis-client receiver, not a substring or
/// a bare identifier, so an unrelated variable named `evalsha` does not trip it.
/// Two forms are recognized:
///   - a dedicated method, `recv.evalsha(...)` / `client.evalSha(...)` /
///     `redis.scriptLoad(...)` / `redis.defineCommand(...)` (see
///     [`REDIS_SCRIPT_CACHE_METHODS`]);
///   - the generic-command form `recv.script('load' | 'exists', ...)`, where the
///     first string argument is a cache-managing SCRIPT subcommand (see
///     [`REDIS_SCRIPT_SUBCOMMANDS`]).
/// Walks the whole AST once; only reached on the rare `createHash('sha1')` path,
/// so the per-file scan is not on the hot path.
fn file_calls_redis_script_cache(semantic: &oxc_semantic::Semantic<'_>) -> bool {
    semantic.nodes().iter().any(|node| {
        let AstKind::CallExpression(call) = node.kind() else {
            return false;
        };
        let method = match &call.callee {
            Expression::StaticMemberExpression(mem) => mem.property.name.as_str(),
            Expression::ComputedMemberExpression(mem) => {
                let Expression::StringLiteral(s) = mem.expression.without_parentheses() else {
                    return false;
                };
                s.value.as_str()
            }
            _ => return false,
        };
        let method = method.to_ascii_lowercase();
        if REDIS_SCRIPT_CACHE_METHODS.contains(&method.as_str()) {
            return true;
        }
        method == "script" && call_first_arg_is_script_cache_subcommand(call)
    })
}

/// True when the first argument of a `.script(...)` call is a string literal that
/// is a cache-managing Redis `SCRIPT` subcommand (see [`REDIS_SCRIPT_SUBCOMMANDS`]).
fn call_first_arg_is_script_cache_subcommand(call: &oxc_ast::ast::CallExpression<'_>) -> bool {
    let Some(first) = call.arguments.first().and_then(|a| a.as_expression()) else {
        return false;
    };
    let Expression::StringLiteral(s) = first.without_parentheses() else {
        return false;
    };
    REDIS_SCRIPT_SUBCOMMANDS.contains(&s.value.to_ascii_lowercase().as_str())
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

    // directus/directus oracle dialect (#5373): SHA-1 shortens an auto-generated
    // index name only when it overflows Oracle's identifier-length cap — a name
    // fingerprint, not a security hash.
    #[test]
    fn allows_sha1_for_length_overflow_index_name() {
        let src = r#"
            function getIndexName(indexName) {
              if (indexName.length > 128) {
                indexName = crypto.createHash('sha1').update(indexName).digest('base64').replace('=', '');
              }
              return indexName;
            }
        "#;
        assert!(run_on(src).is_empty());
    }

    // The flipped comparison form (`N <= x.length`) is the same overflow guard.
    #[test]
    fn allows_sha1_for_flipped_length_overflow_guard() {
        let src = r#"
            const id = name.length >= 63
              ? createHash('sha1').update(name).digest('hex')
              : name;
        "#;
        assert!(run_on(src).is_empty());
    }

    // A full digest with no length-overflow guard is still flagged — the
    // exemption only covers shorten-when-too-long fingerprints.
    #[test]
    fn still_flags_unguarded_full_sha1_digest() {
        let src = "const sig = createHash('sha1').update(payload).digest('hex');";
        assert_eq!(run_on(src).len(), 1);
    }

    // A weak-crypto use whose surrounding `.length` test is an emptiness check
    // (`=== 0`), not an overflow comparison, is still flagged.
    #[test]
    fn still_flags_md5_signature_with_emptiness_length_check() {
        let src = r#"
            if (token.length === 0) {
              throw new Error('empty');
            }
            const sig = createHash('md5').update(token).digest('hex');
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    // bee-queue/bee-queue lib/lua/index.js (#5378): the SHA-1 digest is the Redis
    // EVALSHA script-cache key. The same file manages the cache via the generic
    // `client.script('load'|'exists', ...)` form, which anchors the exemption.
    #[test]
    fn allows_sha1_for_redis_script_cache_via_script_subcommand() {
        let src = r#"
            const hash = crypto.createHash('sha1');
            hash.update(script);
            shas[name] = hash.digest('hex');
            client.script('exists', shas[scriptKey], done);
            client.script('load', scripts[scriptKey], done);
        "#;
        assert!(run_on(src).is_empty());
    }

    // The dedicated `evalsha` / `evalSha` method form anchors the exemption too.
    #[test]
    fn allows_sha1_for_redis_evalsha_method() {
        let src = r#"
            const sha = createHash('sha1').update(script).digest('hex');
            redis.evalsha(sha, 1, key, arg);
        "#;
        assert!(run_on(src).is_empty());
    }

    // A SHA-1 digest with no Redis script-cache call in the file is still flagged:
    // a `.script(...)` call whose subcommand is not cache-managing (`get`) is not
    // an EVALSHA signal.
    #[test]
    fn still_flags_sha1_with_unrelated_script_call() {
        let src = r#"
            const sig = createHash('sha1').update(token).digest('hex');
            editor.script('get', name);
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    // MD5 has no Redis role: a SHA-1-only exemption must not let an MD5 password
    // digest through just because the file also talks to the Redis script cache.
    #[test]
    fn still_flags_md5_password_even_in_redis_script_cache_file() {
        let src = r#"
            const pw = createHash('md5').update(password).digest('hex');
            redis.evalsha(sha, 1, key, arg);
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    // A bare identifier named `evalsha` (not a Redis-client method call) is not a
    // protocol signal — the SHA-1 digest still fires.
    #[test]
    fn still_flags_sha1_with_bare_evalsha_identifier() {
        let src = r#"
            const evalsha = 42;
            const sig = createHash('sha1').update(token).digest('hex');
        "#;
        assert_eq!(run_on(src).len(), 1);
    }
}
