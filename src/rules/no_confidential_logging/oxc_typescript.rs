//! no-confidential-logging OXC backend — flag logging calls, and thrown
//! `Error` messages, containing sensitive identifiers (password, token,
//! apiKey, connectionString, etc.). Secrets interpolated into a thrown
//! `Error` leak into stack traces and error reporters (e.g. Sentry) just
//! as logging them does.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, TemplateLiteral};
use oxc_span::GetSpan;
use std::sync::Arc;

const CONSOLE_METHODS: &[&str] = &["log", "info", "warn", "error", "debug"];

const SENSITIVE_WORDS: &[&str] = &[
    "password",
    "secret",
    "token",
    "apikey",
    "api_key",
    "authorization",
    "credential",
    "ssn",
    "creditcard",
    "credit_card",
    // DB connection strings carry embedded credentials.
    "dsn",
    "connectionstring",
    "connection_string",
    "databaseurl",
    "database_url",
];

/// Path segments identifying documentation/example files that demonstrate an
/// API to library users. Their `console.log` calls show usage with literal
/// placeholder values, not real secrets, so the rule does not apply.
const EXAMPLE_FILE_MARKERS: &[&str] = &["snippet", "example", "sample"];

/// Qualifiers that, adjacent to a `token` word in an identifier, mark it as a
/// transaction/iteration handle rather than a credential. A lock token,
/// continuation token, or page token is a temporary processing handle —
/// like a database cursor — not a secret. `accessToken`/`authToken` carry
/// no such qualifier and remain flagged.
const BENIGN_TOKEN_QUALIFIERS: &[&str] =
    &["lock", "continuation", "cancellation", "page", "next", "reset"];

/// Qualifiers that, adjacent to a `token` word, mark it as NLP/tokenizer
/// vocabulary — a discrete unit of text or a special marker (language tag,
/// modality placeholder, sentinel) — not an authentication secret. `image`
/// (`imageToken`), `lang` (`tgt_lang_token`), and the standard special-token
/// names (`bos`/`eos`/`pad`/`sep`/`cls`/`mask`/`unk`/…) are public model
/// configuration, safe to log. `accessToken`/`apiToken`/`authToken` carry no
/// NLP qualifier and remain flagged.
const NLP_TOKEN_QUALIFIERS: &[&str] = &[
    "lang", "image", "audio", "video", "vision", "bos", "eos", "pad", "sep", "cls", "mask", "unk",
    "vocab", "special", "sentinel", "start", "end", "sub", "subword", "word", "byte",
];

/// Words that mark a `…token` identifier as an authentication/authorization
/// credential. Their presence *anywhere* in the identifier vetoes the benign
/// exemption, even when the word adjacent to `token` is an NLP qualifier:
/// `accessWordToken` (`access` + `Word` + `Token`) is an access token, not
/// vocabulary, and must stay flagged.
const CREDENTIAL_TOKEN_QUALIFIERS: &[&str] = &[
    "access", "refresh", "auth", "api", "bearer", "id", "csrf", "xsrf", "session", "jwt", "oauth",
    "secret", "private",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::CallExpression,
            AstType::NewExpression,
            AstType::ThrowStatement,
        ]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["console", "logger", "throw", "Error"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if is_example_file(ctx.path) {
            return;
        }

        let leak = match node.kind() {
            AstKind::CallExpression(call) => {
                (is_logging_callee(&call.callee)
                    && has_sensitive_argument(&call.arguments, ctx.source))
                .then_some((call.span.start, LeakKind::Logging))
            }
            AstKind::NewExpression(new_expr) => {
                (is_error_constructor(&new_expr.callee)
                    && has_sensitive_template_argument(&new_expr.arguments, ctx.source))
                .then_some((new_expr.span.start, LeakKind::ThrownError))
            }
            AstKind::ThrowStatement(throw) => match &throw.argument {
                // `throw new Error(...)` is reached via the NewExpression arm.
                Expression::TemplateLiteral(tpl) => {
                    template_has_sensitive_substitution(tpl, ctx.source)
                        .then_some((throw.span.start, LeakKind::ThrownError))
                }
                _ => None,
            },
            _ => None,
        };

        let Some((offset, kind)) = leak else {
            return;
        };

        let (line, column) = byte_offset_to_line_col(ctx.source, offset as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: kind.message().into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// Which sink leaked a secret — selects the diagnostic message.
enum LeakKind {
    Logging,
    ThrownError,
}

impl LeakKind {
    fn message(&self) -> &'static str {
        match self {
            LeakKind::Logging => {
                "Logging call contains sensitive data — redact secrets before logging."
            }
            LeakKind::ThrownError => {
                "Thrown error message contains sensitive data — secrets leak into stack \
                 traces and error reporters. Redact secrets before throwing."
            }
        }
    }
}

/// `Error`, or a `*Error` subclass (`TypeError`, `CustomError`, …). Matches the
/// constructor of a `new` expression by identifier name only.
fn is_error_constructor(callee: &Expression) -> bool {
    let Expression::Identifier(id) = callee else {
        return false;
    };
    let name = id.name.as_str();
    name == "Error" || name.ends_with("Error")
}

fn is_logging_callee(callee: &Expression) -> bool {
    let Expression::StaticMemberExpression(member) = callee else {
        return false;
    };
    let obj_name = match &member.object {
        Expression::Identifier(id) => id.name.as_str(),
        _ => return false,
    };
    let prop = member.property.name.as_str();

    if obj_name == "console" && CONSOLE_METHODS.contains(&prop) {
        return true;
    }
    if obj_name == "logger" {
        return true;
    }
    false
}

fn has_sensitive_argument(args: &[Argument], source: &str) -> bool {
    for arg in args {
        match arg {
            Argument::StringLiteral(_) => continue,
            Argument::TemplateLiteral(tpl) => {
                if template_has_sensitive_substitution(tpl, source) {
                    return true;
                }
            }
            _ => {
                let span = arg.span();
                if text_is_sensitive(&source[span.start as usize..span.end as usize]) {
                    return true;
                }
            }
        }
    }
    false
}

/// Like [`has_sensitive_argument`] but restricted to template-literal
/// interpolations. A thrown `Error` only leaks a secret when the message
/// *interpolates* one (`new Error(`…${secret}`)`); a plain string literal
/// (`new Error("db failed")`) or a passed-through identifier carries no
/// inline secret, so the error branch deliberately ignores them.
fn has_sensitive_template_argument(args: &[Argument], source: &str) -> bool {
    args.iter().any(|arg| match arg {
        Argument::TemplateLiteral(tpl) => template_has_sensitive_substitution(tpl, source),
        _ => false,
    })
}

fn template_has_sensitive_substitution(tpl: &TemplateLiteral, source: &str) -> bool {
    tpl.expressions
        .iter()
        .any(|expr| interpolation_is_sensitive(expr, source))
}

/// Whether a single template interpolation can leak a secret value.
///
/// The check is structural, not a name allowlist: an expression is exempt only
/// when its *type* provably cannot carry the secret's bytes.
///  - A ternary's `test` is consumed as a boolean — its bytes never reach the
///    produced string — so only the `consequent`/`alternate` branches (the
///    actual output) are examined. `${hasToken() ? "available" : "not found"}`
///    is safe; `${cond ? accessToken : ""}` still flags on the consequent.
///  - A provably-boolean expression (comparison, `!x`, `Boolean(...)`, boolean
///    literal) reveals only presence/validity, never the value, so
///    `${tokenLength === 32}`-style predicates are exempt.
///
/// Anything else — a bare identifier, a member access (`obj.token`), an
/// unresolvable call — falls through to the name-based text check and still
/// flags. Precision is in the safe direction: suppress only when boolean-ness
/// is proven.
fn interpolation_is_sensitive(expr: &Expression, source: &str) -> bool {
    match crate::oxc_helpers::peel_parens(expr) {
        Expression::ConditionalExpression(cond) => {
            interpolation_is_sensitive(&cond.consequent, source)
                || interpolation_is_sensitive(&cond.alternate, source)
        }
        peeled if is_boolean_expression(peeled) => false,
        peeled => {
            let span = peeled.span();
            text_is_sensitive(&source[span.start as usize..span.end as usize])
        }
    }
}

/// True when `expr` provably produces a boolean: a comparison, a logical
/// negation, a `Boolean(...)` coercion, or a boolean literal. Such a value
/// reveals only true/false and cannot carry a secret. A logical `&&`/`||` is
/// deliberately excluded — JS short-circuit returns an operand, not a boolean
/// (`flag && rawToken` yields the token), so it is not provably boolean.
fn is_boolean_expression(expr: &Expression) -> bool {
    use oxc_ast::ast::{BinaryOperator, UnaryOperator};
    match expr {
        Expression::BooleanLiteral(_) => true,
        Expression::UnaryExpression(unary) => unary.operator == UnaryOperator::LogicalNot,
        Expression::BinaryExpression(bin) => matches!(
            bin.operator,
            BinaryOperator::Equality
                | BinaryOperator::StrictEquality
                | BinaryOperator::Inequality
                | BinaryOperator::StrictInequality
                | BinaryOperator::LessThan
                | BinaryOperator::LessEqualThan
                | BinaryOperator::GreaterThan
                | BinaryOperator::GreaterEqualThan
                | BinaryOperator::In
                | BinaryOperator::Instanceof
        ),
        Expression::CallExpression(call) => {
            matches!(&call.callee, Expression::Identifier(id) if id.name == "Boolean")
        }
        _ => false,
    }
}

fn is_example_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    EXAMPLE_FILE_MARKERS.iter().any(|m| s.contains(m))
}

/// Returns true when an interpolated expression names a secret. Matching is
/// per identifier segment (split on non-`[a-z0-9_]`) so a benign `…token`
/// compound such as `lockToken` is not flagged on the strength of an
/// unrelated neighbour. The original case is preserved here so the segment can
/// be split at camelCase boundaries downstream.
fn text_is_sensitive(text: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|seg| !seg.is_empty())
        .any(segment_is_sensitive)
}

fn segment_is_sensitive(segment: &str) -> bool {
    let lower = segment.to_ascii_lowercase();
    SENSITIVE_WORDS.iter().any(|w| {
        // `dsn` is only three characters and substring-collides with a whole
        // family of benign `…dsName` identifiers (`fieldsName`, `boundsName`,
        // `kidsName`, …). Anchor it to the segment suffix so it matches a DSN
        // (`dsn`, `dbDsn`, `sentryDsn`) but not those neighbours.
        if *w == "dsn" {
            return lower.ends_with("dsn");
        }
        if !lower.contains(w) {
            return false;
        }
        if *w == "token" && is_benign_token(segment) {
            return false;
        }
        true
    })
}

/// A `…token` identifier whose qualifier word marks it as something other than
/// a credential: a transaction/iteration handle (`lockToken`, `pageToken`) or
/// NLP/tokenizer vocabulary (`imageToken`, `tgt_lang_token`, `bos_token`).
/// Words are obtained by splitting at camelCase and `_` boundaries.
///
/// Biasing toward the warning, a token is benign only when ALL of:
///  - it has at least one neighbour word (so bare `token` stays flagged);
///  - every neighbour of the `token` word is a recognized benign qualifier;
///  - no word *anywhere* in the identifier is a credential qualifier — a
///    credential word vetoes the exemption even when not adjacent to `token`,
///    so `accessWordToken` and `accessTokenImage` stay flagged.
fn is_benign_token(segment: &str) -> bool {
    let words: Vec<&str> = split_identifier_words(segment).collect();
    let has_credential_word = words.iter().any(|w| {
        CREDENTIAL_TOKEN_QUALIFIERS
            .iter()
            .any(|q| w.eq_ignore_ascii_case(q))
    });
    if has_credential_word {
        return false;
    }
    let mut benign_qualifier_found = false;
    for (i, word) in words.iter().enumerate() {
        if !word.eq_ignore_ascii_case("token") {
            continue;
        }
        let neighbours: Vec<&str> = [i.checked_sub(1).map(|p| words[p]), words.get(i + 1).copied()]
            .into_iter()
            .flatten()
            .collect();
        let all_neighbours_benign = !neighbours.is_empty()
            && neighbours.iter().all(|n| {
                BENIGN_TOKEN_QUALIFIERS
                    .iter()
                    .chain(NLP_TOKEN_QUALIFIERS)
                    .any(|q| n.eq_ignore_ascii_case(q))
            });
        if !all_neighbours_benign {
            return false;
        }
        benign_qualifier_found = true;
    }
    benign_qualifier_found
}

/// Splits an identifier into word tokens at camelCase boundaries and any
/// non-alphanumeric separator (`_`, `-`, `.`). `imageToken` → `image`, `Token`;
/// `tgt_lang_token` → `tgt`, `lang`, `token`.
fn split_identifier_words(name: &str) -> impl Iterator<Item = &str> {
    let bytes = name.as_bytes();
    let mut start = 0;
    let mut boundaries = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        let is_sep = !b.is_ascii_alphanumeric();
        let is_camel_boundary =
            i > 0 && b.is_ascii_uppercase() && bytes[i - 1].is_ascii_lowercase();
        if is_sep {
            if start < i {
                boundaries.push((start, i));
            }
            start = i + 1;
        } else if is_camel_boundary {
            boundaries.push((start, i));
            start = i;
        }
    }
    if start < bytes.len() {
        boundaries.push((start, bytes.len()));
    }
    boundaries.into_iter().map(move |(s, e)| &name[s..e])
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_console_log_with_password() {
        assert_eq!(run_on("console.log('label:', config.password);").len(), 1);
    }

    #[test]
    fn flags_console_error_with_token() {
        assert_eq!(run_on("console.error(`token=${accessToken}`);").len(), 1);
    }

    #[test]
    fn flags_logger_with_api_key() {
        assert_eq!(run_on("logger.info('label:', apiKey);").len(), 1);
    }

    #[test]
    fn allows_logging_without_sensitive_data() {
        assert!(run_on("console.log('User logged in');").is_empty());
    }

    #[test]
    fn allows_string_literal_mentioning_token() {
        assert!(run_on(r#"console.log("Token refresh succeeded");"#).is_empty());
    }

    // Regression #1105 — `snippets.spec.ts` is a documentation file whose
    // console.log calls demonstrate an API with placeholder values.
    #[test]
    fn allows_logging_in_snippet_file() {
        let src = "console.log(credential.key);";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "test/snippets.spec.ts").is_empty()
        );
    }

    #[test]
    fn allows_logging_in_samples_dir() {
        let src = "console.log(credential.key);";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, src, "samples/demo.ts").is_empty()
        );
    }

    // Regression #1105 — a lock token is a transaction handle, not a secret.
    #[test]
    fn allows_lock_token_in_error_log() {
        let src =
            "logger.logError(err, `An error occurred while auto renewing the message lock '${bMessage.lockToken}'`);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_continuation_token() {
        assert!(run_on("logger.info(`page from ${result.continuationToken}`);").is_empty());
    }

    // Still-flags controls: genuine secret tokens must keep firing.
    #[test]
    fn still_flags_access_token() {
        assert_eq!(run_on("logger.info(`auth: ${accessToken}`);").len(), 1);
    }

    #[test]
    fn still_flags_lock_token_outside_example_file() {
        // The benign-token exemption is about the *name*, not the file: a real
        // secret token in the same file still fires.
        assert_eq!(run_on("logger.info(`token: ${authToken}`);").len(), 1);
    }

    // #5410 — NLP/tokenizer "token" terminology is model vocabulary, not a
    // secret: language tags and modality placeholders are public config and
    // are logged in validation errors so a developer can debug bad input.
    #[test]
    fn allows_nlp_lang_token_in_error() {
        let src = "throw new Error(`Target language code \"${tgt_lang_token}\" is not valid.`);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_nlp_src_lang_token() {
        assert!(run_on("logger.info(`routing ${src_lang_token}`);").is_empty());
    }

    #[test]
    fn allows_nlp_image_token() {
        let src =
            "throw new Error(`The text does not contain the image token ${image_token}.`);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_nlp_special_token_camel_case() {
        assert!(run_on("console.log(`placeholder ${imageToken}`);").is_empty());
    }

    #[test]
    fn allows_nlp_bos_token() {
        assert!(run_on("logger.info(`bos ${bos_token} eos ${eos_token}`);").is_empty());
    }

    // Security controls: genuine auth/secret tokens must still fire, even
    // alongside the NLP exemption.
    #[test]
    fn still_flags_access_token_with_nlp_exemption() {
        assert_eq!(run_on("logger.info(`auth: ${access_token}`);").len(), 1);
    }

    #[test]
    fn still_flags_api_token_camel_case() {
        assert_eq!(run_on("logger.info(`auth: ${apiToken}`);").len(), 1);
    }

    #[test]
    fn still_flags_bare_token() {
        assert_eq!(run_on("logger.info(`auth: ${token}`);").len(), 1);
    }

    // A credential qualifier on either side keeps the token flagged even when
    // the other neighbour is a benign NLP word: an access token for an image
    // API is still a secret.
    #[test]
    fn still_flags_access_token_with_nlp_neighbour() {
        assert_eq!(run_on("logger.info(`auth: ${access_token_image}`);").len(), 1);
        assert_eq!(run_on("logger.info(`auth: ${accessTokenImage}`);").len(), 1);
    }

    // A credential word vetoes the exemption even when it is not adjacent to
    // `token`: `accessWordToken` is an access token, not vocabulary.
    #[test]
    fn still_flags_credential_word_not_adjacent_to_token() {
        assert_eq!(run_on("logger.info(`auth: ${accessWordToken}`);").len(), 1);
        assert_eq!(run_on("logger.info(`auth: ${auth_lang_token}`);").len(), 1);
    }

    // #3769 — secrets in thrown Error messages leak into stack traces and
    // error reporters (Sentry) just as logging them does.
    #[test]
    fn flags_throw_new_error_with_connection_string() {
        assert_eq!(
            run_on("throw new Error(`Failed to connect: ${connectionString}`);").len(),
            1
        );
    }

    #[test]
    fn flags_throw_custom_error_with_api_key() {
        assert_eq!(run_on("throw new CustomError(`token=${apiKey}`);").len(), 1);
    }

    #[test]
    fn flags_bare_throw_template_with_password() {
        assert_eq!(run_on("throw `secret ${password}`;").len(), 1);
    }

    #[test]
    fn flags_new_error_with_dsn() {
        assert_eq!(run_on("throw new Error(`bad dsn ${dsn}`);").len(), 1);
    }

    // `dsn` is anchored to the segment suffix: a `…dsName` identifier
    // (`fieldsName`) substring-contains "dsn" but is not a connection DSN.
    #[test]
    fn allows_ds_name_identifier() {
        assert!(run_on("logger.info(`fields: ${fieldsName}`);").is_empty());
    }

    // A constructed-but-not-yet-thrown Error still leaks once reported.
    #[test]
    fn flags_constructed_error_with_database_url() {
        assert_eq!(
            run_on("const e = new TypeError(`db ${databaseUrl}`);").len(),
            1
        );
    }

    #[test]
    fn allows_throw_error_with_string_literal() {
        assert!(run_on(r#"throw new Error("Database connection failed");"#).is_empty());
    }

    #[test]
    fn allows_throw_error_with_benign_page_token() {
        assert!(run_on("throw new Error(`page ${pageToken}`);").is_empty());
    }

    // A passed-through identifier is not an inline secret: the error branch
    // only fires on template-literal interpolations.
    #[test]
    fn allows_throw_error_with_identifier_argument() {
        assert!(run_on("throw new Error(password);").is_empty());
    }

    // The new throw/Error branch is reachable in a source with no logging
    // sink, proving the prefilter trigger was extended.
    #[test]
    fn fires_in_throw_only_source() {
        let src = "function f() { throw new Error(`db: ${connectionString}`); }";
        assert_eq!(run_on(src).len(), 1);
    }

    // #5687 — a boolean predicate call whose name contains "token", used in a
    // ternary that produces only diagnostic string literals, logs presence not
    // the secret value. The ternary test is consumed as a boolean.
    #[test]
    fn allows_boolean_predicate_availability_ternary() {
        let src = r#"console.log(
            `github host auth: ${hasHostGitHubToken() ? "available" : "not found"}`
        );"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_boolean_predicate_oidc_token_ternary() {
        let src = r#"console.log(
            `vercel oidc token: ${hasHostVercelOidcToken() ? "available" : "not found"}`
        );"#;
        assert!(run_on(src).is_empty());
    }

    // A comparison/negation interpolation reveals only validity, not the value.
    #[test]
    fn allows_boolean_comparison_interpolation() {
        assert!(run_on("console.log(`valid: ${tokenLength === 32}`);").is_empty());
        assert!(run_on("console.log(`missing: ${!authToken}`);").is_empty());
        assert!(run_on("console.log(`present: ${Boolean(apiToken)}`);").is_empty());
    }

    // Security control: the actual token STRING value still flags even when a
    // ternary wraps it — the consequent carries the secret's bytes.
    #[test]
    fn still_flags_token_value_in_ternary_branch() {
        assert_eq!(
            run_on(r#"console.log(`auth: ${cond ? accessToken : ""}`);"#).len(),
            1
        );
    }

    // Security control: a member access (`obj.token`) is not provably boolean,
    // so it still flags — the name-based check is the fallback.
    #[test]
    fn still_flags_member_access_token() {
        assert_eq!(run_on("console.log(`auth: ${config.accessToken}`);").len(), 1);
    }

    // Security control: a bare predicate call is not type-resolved to boolean,
    // so it falls through to the name check and still flags. Suppression
    // requires proven boolean-ness (here, the ternary-test position).
    #[test]
    fn still_flags_bare_token_call_outside_ternary() {
        assert_eq!(run_on("console.log(`auth: ${getAccessToken()}`);").len(), 1);
    }

    // Security control: a logical `&&`/`||` is not provably boolean — JS
    // short-circuit returns an operand, so `flag && accessToken` yields the
    // token. It must still flag.
    #[test]
    fn still_flags_logical_operand_token() {
        assert_eq!(run_on("console.log(`auth: ${flag && accessToken}`);").len(), 1);
        assert_eq!(run_on("console.log(`auth: ${accessToken || fallback}`);").len(), 1);
    }

    // #4733 — a `*.spec.ts` test that passes sentinel secrets to a logger to
    // verify the logger redacts them is not a leak. The sentinel values are
    // test data, not production secrets, so the rule is skipped in test files.
    #[test]
    fn allows_redaction_test_in_spec_file() {
        let src = r#"
            logger.info(
              { user: "testuser", password: "secretpassword123", token: "bearer-token-xyz" },
              "User authentication attempt"
            );
            expect(logEntry.password).toBe("[Redacted]");
            expect(logEntry.token).toBe("[Redacted]");
        "#;
        assert!(
            crate::rules::test_helpers::run_rule_gated(
                &Check,
                src,
                "packages/logger/test/logger.spec.ts"
            )
            .is_empty()
        );
    }

    // Control: the same sensitive logging in production code still fires
    // through the production gate.
    #[test]
    fn still_flags_in_production_file_through_gate() {
        let src = "logger.info(`auth: ${accessToken}`);";
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(&Check, src, "src/auth.ts").len(),
            1
        );
    }
}
