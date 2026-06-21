//! ban-dependencies OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

const BANNED: &[(&str, &str)] = &[
    ("lodash", "Use native methods or es-toolkit"),
    ("lodash-es", "Use native methods or es-toolkit"),
    ("underscore", "Use native methods or es-toolkit"),
    ("moment", "Use date-fns or Temporal"),
    ("moment-timezone", "Use date-fns-tz or Temporal"),
    ("request", "Use fetch or undici"),
    ("request-promise", "Use fetch or undici"),
    ("bluebird", "Use native Promises"),
    ("q", "Use native Promises"),
    ("left-pad", "Use String.prototype.padStart"),
    ("is-number", "Use typeof or Number.isFinite"),
    ("is-string", "Use typeof"),
    ("is-array", "Use Array.isArray"),
];

fn extract_package_name(specifier: &str) -> String {
    if specifier.starts_with('@') {
        specifier.splitn(3, '/').take(2).collect::<Vec<_>>().join("/")
    } else {
        specifier.split('/').next().unwrap_or(specifier).to_string()
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (specifier, span) = match node.kind() {
            AstKind::ImportDeclaration(import) => {
                (import.source.value.as_str(), import.span)
            }
            AstKind::CallExpression(call) => {
                // require('...')
                let is_require = match &call.callee {
                    oxc_ast::ast::Expression::Identifier(id) => id.name.as_str() == "require",
                    _ => false,
                };
                if !is_require {
                    return;
                }
                let Some(first_arg) = call.arguments.first() else {
                    return;
                };
                let oxc_ast::ast::Argument::StringLiteral(s) = first_arg else {
                    return;
                };
                (s.value.as_str(), call.span)
            }
            _ => return,
        };

        let pkg = extract_package_name(specifier);

        for (banned, reason) in BANNED {
            if pkg == *banned {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!("'{}' is banned. {}", banned, reason),
                    severity: Severity::Warning,
                    span: None,
                });
                return;
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
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.js")
    }

    #[test]
    fn allows_caolan_async_require() {
        // caolan/async is a maintained callback flow-control library; it is the
        // correct tool when the orchestrated code is inherently callback-based.
        assert!(run("var async = require('async');").is_empty());
        assert!(run("async.each(ops, function(op, cb) {}, done);").is_empty());
    }

    #[test]
    fn allows_caolan_async_import() {
        assert!(run("import async from 'async';").is_empty());
    }

    #[test]
    fn still_flags_deprecated_request() {
        let d = run("var request = require('request');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("'request' is banned"));
    }

    #[test]
    fn still_flags_moment() {
        let d = run("import moment from 'moment';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("'moment' is banned"));
    }
}
