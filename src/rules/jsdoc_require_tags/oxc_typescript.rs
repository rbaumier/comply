//! jsdoc/require-tags OXC backend — require `@param` / `@returns` when
//! relevant on exported functions.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, has_tag, parse_tags};
use std::sync::Arc;

pub struct Check;

fn exported_function_signature(code: &str) -> Option<String> {
    let sig: String = code.lines().take(4).collect::<Vec<_>>().join(" ");
    let t = sig.trim();
    if !(t.starts_with("export function ")
        || t.starts_with("export async function ")
        || t.starts_with("export default function ")
        || t.starts_with("export default async function ")
        || t.starts_with("export const ")
        || t.starts_with("export let "))
    {
        return None;
    }
    Some(sig)
}

fn signature_has_params(sig: &str) -> bool {
    let open = match sig.find('(') {
        Some(i) => i,
        None => return false,
    };
    let close = sig[open..].find(')').map(|i| open + i).unwrap_or(sig.len());
    let between = &sig[open + 1..close];
    !between.trim().is_empty()
}

fn signature_has_non_void_return(sig: &str) -> bool {
    let after_paren = match sig.find(')') {
        Some(i) => &sig[i + 1..],
        None => return false,
    };
    let end = after_paren
        .find('{')
        .or_else(|| after_paren.find("=>"))
        .unwrap_or(after_paren.len());
    let ret_section = after_paren[..end].trim();
    let ret = match ret_section.strip_prefix(':') {
        Some(r) => r.trim(),
        None => return false,
    };
    !(ret.is_empty()
        || ret == "void"
        || ret == "Promise<void>"
        || ret.starts_with("void ")
        || ret.starts_with("Promise<void>"))
}

/// Reconstruct the source text following a JSDoc comment block.
fn following_code_from_end(source: &str, comment_end: usize) -> &str {
    &source[comment_end..]
}

impl OxcCheck for Check {
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
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            // OXC comment spans exclude `/*` prefix — real start is 2 bytes before.
            if start < 2 {
                continue;
            }
            let doc_start = start - 2;
            // Include the closing `*/` — end is already past the content.
            let raw_end = (end + 2).min(ctx.source.len());
            let Some(text) = ctx.source.get(doc_start..raw_end) else {
                continue;
            };
            if !text.starts_with("/**") {
                continue;
            }

            let (base_line, _) = byte_offset_to_line_col(ctx.source, doc_start);

            for block in find_jsdoc_blocks(text) {
                let code = following_code_from_end(ctx.source, raw_end);
                let sig = match exported_function_signature(code) {
                    Some(s) => s,
                    None => continue,
                };
                let tags = parse_tags(&block.content);

                if signature_has_params(&sig) && !has_tag(&tags, "param") {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: base_line + block.start_line,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: "Exported function has parameters but no `@param` tags in its JSDoc."
                            .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
                if signature_has_non_void_return(&sig)
                    && !has_tag(&tags, "returns")
                    && !has_tag(&tags, "return")
                {
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line: base_line + block.start_line,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message:
                            "Exported function returns a value but JSDoc has no `@returns` tag."
                                .into(),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }


    #[test]
    fn allows_exported_fn_with_param_and_returns() {
        let src = "/**\n * @param x - input\n * @returns output\n */\nexport function f(x: number): number { return x; }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_exported_fn() {
        let src = "/**\n * internal\n */\nfunction f(x: number): number { return x; }";
        assert!(run(src).is_empty());
    }
}
