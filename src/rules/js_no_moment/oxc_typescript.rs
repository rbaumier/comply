//! js-no-moment oxc backend — flag `import ... from 'moment'` and `require('moment')`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const MESSAGE: &str = "moment.js is 300kB+ \u{2014} use `date-fns`, `dayjs`, or `Temporal`.";

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration, AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["moment"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match node.kind() {
            AstKind::ImportDeclaration(import) => {
                if import.source.value.as_str() != "moment" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, import.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: MESSAGE.into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            AstKind::CallExpression(call) => {
                let Expression::Identifier(callee) = &call.callee else { return };
                if callee.name.as_str() != "require" {
                    return;
                }
                let Some(arg) = call.arguments.first() else { return };
                let Some(expr) = arg.as_expression() else { return };
                let Expression::StringLiteral(lit) = expr else { return };
                if lit.value.as_str() != "moment" {
                    return;
                }
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: MESSAGE.into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_default_import() {
        assert_eq!(run(r#"import moment from 'moment';"#).len(), 1);
    }


    #[test]
    fn flags_namespace_import() {
        assert_eq!(run(r#"import * as moment from 'moment';"#).len(), 1);
    }


    #[test]
    fn flags_require_call() {
        assert_eq!(run(r#"const moment = require('moment');"#).len(), 1);
    }


    #[test]
    fn allows_dayjs_import() {
        assert!(run(r#"import dayjs from 'dayjs';"#).is_empty());
    }


    #[test]
    fn allows_date_fns_import() {
        assert!(run(r#"import { format } from 'date-fns';"#).is_empty());
    }


    #[test]
    fn allows_unrelated_require() {
        assert!(run(r#"const fs = require('fs');"#).is_empty());
    }
}
