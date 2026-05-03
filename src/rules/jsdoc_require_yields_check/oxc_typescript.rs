use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, has_tag, parse_tags};
use std::sync::Arc;

pub struct Check;

fn function_body_after(source: &str, block_raw: &str) -> String {
    let idx = match source.find(block_raw) {
        Some(i) => i + block_raw.len(),
        None => return String::new(),
    };
    let tail = &source[idx..];
    let mut out = String::new();
    let mut lines = 0;
    for line in tail.lines() {
        out.push_str(line);
        out.push('\n');
        lines += 1;
        if lines >= 40 {
            break;
        }
    }
    out
}

fn body_has_yield(code: &str) -> bool {
    code.split_whitespace().any(|tok| {
        tok == "yield" || tok == "yield;" || tok == "yield*" || tok.starts_with("yield(")
    }) || code.contains(" yield ")
        || code.contains("\tyield ")
        || code.contains("\nyield ")
}

fn is_generator_signature(code: &str) -> bool {
    code.contains("function*") || code.contains("function *")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        for comment in semantic.comments() {
            let raw = &ctx.source[comment.span.start as usize..comment.span.end as usize];
            if !raw.starts_with("/**") {
                continue;
            }

            let line_offset = byte_offset_to_line_col(ctx.source, comment.span.start as usize).0;

            for block in find_jsdoc_blocks(raw) {
                let tags = parse_tags(&block.content);
                let has_yields_tag = has_tag(&tags, "yields");
                let body = function_body_after(ctx.source, raw);
                let is_gen = is_generator_signature(&body);
                let yields_in_body = body_has_yield(&body);

                if has_yields_tag && !yields_in_body {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: block.start_line + line_offset,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "`@yields` is documented but the function does not yield — remove the tag.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                } else if is_gen && yields_in_body && !has_yields_tag {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: block.start_line + line_offset,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "Function yields but JSDoc is missing `@yields` — document what it yields.".into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}
