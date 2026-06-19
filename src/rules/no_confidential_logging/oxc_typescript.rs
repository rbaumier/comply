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

/// Qualifiers that, when they prefix a `…token` identifier, mark it as a
/// transaction/iteration handle rather than a credential. A lock token,
/// continuation token, or page token is a temporary processing handle —
/// like a database cursor — not a secret. `accessToken`/`authToken` carry
/// no such qualifier and remain flagged.
const BENIGN_TOKEN_PREFIXES: &[&str] =
    &["lock", "continuation", "cancellation", "page", "next", "reset"];

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
    for expr in &tpl.expressions {
        let span = expr.span();
        if text_is_sensitive(&source[span.start as usize..span.end as usize]) {
            return true;
        }
    }
    false
}

fn is_example_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    EXAMPLE_FILE_MARKERS.iter().any(|m| s.contains(m))
}

/// Returns true when an interpolated expression names a secret. Matching is
/// per identifier segment (split on non-`[a-z0-9_]`) so a benign `…token`
/// compound such as `lockToken` is not flagged on the strength of an
/// unrelated neighbour.
fn text_is_sensitive(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|seg| !seg.is_empty())
        .any(segment_is_sensitive)
}

fn segment_is_sensitive(segment: &str) -> bool {
    SENSITIVE_WORDS.iter().any(|w| {
        // `dsn` is only three characters and substring-collides with a whole
        // family of benign `…dsName` identifiers (`fieldsName`, `boundsName`,
        // `kidsName`, …). Anchor it to the segment suffix so it matches a DSN
        // (`dsn`, `dbDsn`, `sentryDsn`) but not those neighbours.
        if *w == "dsn" {
            return segment.ends_with("dsn");
        }
        if !segment.contains(w) {
            return false;
        }
        if *w == "token" && is_benign_token(segment) {
            return false;
        }
        true
    })
}

/// `lockToken`, `continuationToken`, `pageToken`, … — a `…token` identifier
/// whose qualifier marks it as a transaction or iteration handle, not a
/// credential.
fn is_benign_token(segment: &str) -> bool {
    let Some(prefix) = segment.strip_suffix("token") else {
        return false;
    };
    let prefix = prefix.trim_end_matches('_');
    BENIGN_TOKEN_PREFIXES.contains(&prefix)
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
}
