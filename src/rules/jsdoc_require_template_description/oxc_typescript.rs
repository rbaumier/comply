//! jsdoc/require-template-description oxc backend — every `@template` tag
//! needs a description.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use crate::rules::jsdoc_text_helpers::{find_jsdoc_blocks, parse_tags, strip_type_annotation};
use std::sync::Arc;

fn template_has_description(value: &str) -> bool {
    let rest = strip_type_annotation(value.trim());
    if let Some(idx) = rest.find(" - ") {
        return !rest[idx + 3..].trim().is_empty();
    }
    let first_non_ident = rest
        .char_indices()
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == ',' || *c == ' '))
        .map(|(i, _)| i);
    if let Some(i) = first_non_ident {
        !rest[i..].trim().is_empty()
    } else {
        false
    }
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // Scan source for JSDoc blocks manually (comments are not in the oxc AST).
        let src = ctx.source;
        let mut search_from = 0;
        while let Some(start) = src[search_from..].find("/**") {
            let abs_start = search_from + start;
            let Some(end_rel) = src[abs_start + 3..].find("*/") else { break };
            let abs_end = abs_start + 3 + end_rel + 2;
            let comment_text = &src[abs_start..abs_end];
            let line_offset = src[..abs_start].matches('\n').count();

            for block in find_jsdoc_blocks(comment_text) {
                for tag in parse_tags(&block.content) {
                    if tag.name != "template" {
                        continue;
                    }
                    if !template_has_description(&tag.value) {
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line: block.start_line + tag.line_offset + 1 + line_offset,
                            column: 1,
                            rule_id: super::META.id.into(),
                            message: "@template tag is missing a description — document the type parameter.".into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
            search_from = abs_end;
        }
        diagnostics
    }
}
