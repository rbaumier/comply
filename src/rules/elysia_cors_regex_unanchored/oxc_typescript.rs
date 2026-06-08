//! elysia-cors-regex-unanchored oxc backend — flag CORS regex origin missing trailing `$`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !ctx.project.has_framework("elysia") {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::Identifier(id) = &call.callee else { return };
        if id.name.as_str() != "cors" {
            return;
        }

        // Inspect the arguments text for `origin:` followed by a regex literal.
        let args_start = call.span.start as usize;
        let args_end = call.span.end as usize;
        let args_text = &ctx.source[args_start..args_end];

        let mut idx = 0;
        let bytes = args_text.as_bytes();
        while idx < bytes.len() {
            if let Some(rest) = args_text.get(idx..)
                && let Some(off) = rest.find("origin:") {
                    let after = &rest[off + "origin:".len()..];
                    let after_trim = after.trim_start();
                    if let Some(body) = after_trim.strip_prefix('/') {
                        let mut end = None;
                        let mut esc = false;
                        for (i, c) in body.char_indices() {
                            if esc { esc = false; continue; }
                            if c == '\\' { esc = true; continue; }
                            if c == '/' { end = Some(i); break; }
                        }
                        if let Some(e) = end {
                            let regex_body = &body[..e];
                            if !regex_body.ends_with('$') {
                                let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line,
                                    column,
                                    rule_id: super::META.id.into(),
                                    message: "CORS origin regex is not anchored with `$` — may match unintended origins.".into(),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                        }
                    }
                    idx += off + "origin:".len();
                    continue;
                }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_unanchored_regex() {
        let src =
            "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: /example\\.com/ }));";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_anchored_regex() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ origin: /^https:\\/\\/example\\.com$/ }));";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.use(cors({ origin: /example\\.com/ }));";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
