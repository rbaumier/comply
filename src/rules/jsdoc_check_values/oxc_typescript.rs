//! jsdoc/check-values oxc backend — validate `@version`, `@since`, `@license`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::jsdoc_helpers::scan_blocks;
use std::sync::Arc;

pub struct Check;

/// Very lax semver check — `1`, `1.2`, `1.2.3`, `1.2.3-rc.1`, `v1.2`.
fn is_semverish(s: &str) -> bool {
    let mut chars = s.chars();
    let first = chars.next();
    let after_v = match first {
        Some('v') | Some('V') => chars.next(),
        Some(c) => Some(c),
        None => return false,
    };
    match after_v {
        Some(c) if c.is_ascii_digit() => {}
        _ => return false,
    }
    s.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
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
            if start < 2 {
                continue;
            }
            let doc_start = start - 2;
            let Some(raw) = ctx.source.get(doc_start..end) else {
                continue;
            };
            if !raw.starts_with("/**") {
                continue;
            }

            let (base_line, _) = byte_offset_to_line_col(ctx.source, doc_start);

            for block in scan_blocks(raw) {
                for tag in block.tags() {
                    match tag.name.as_str() {
                        "version" | "since" => {
                            let body = tag.body.trim();
                            if !is_semverish(body) {
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line: tag.line + base_line,
                                    column: 1,
                                    rule_id: super::META.id.into(),
                                    message: format!(
                                        "`@{}` value `{}` is not a semver-ish string (expected e.g. `1.2.3`).",
                                        tag.name, body
                                    ),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                        }
                        "license" => {
                            let body = tag.body.trim();
                            if body.is_empty() {
                                diagnostics.push(Diagnostic {
                                    path: Arc::clone(&ctx.path_arc),
                                    line: tag.line + base_line,
                                    column: 1,
                                    rule_id: super::META.id.into(),
                                    message:
                                        "`@license` tag has no value — add an SPDX identifier like `MIT`."
                                            .into(),
                                    severity: Severity::Warning,
                                    span: None,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        diagnostics
    }
}
