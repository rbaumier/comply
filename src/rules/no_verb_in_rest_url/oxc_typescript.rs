//! no-verb-in-rest-url oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::StringLiteral, AstType::TemplateLiteral]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/api/", "/v1/", "/v2/"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Import/export source strings (`from "./api/getThing.js"`) are file
        // paths, not REST URLs — never flag them.
        let parent = semantic.nodes().parent_node(node.id());
        if matches!(
            parent.kind(),
            AstKind::ImportDeclaration(_)
                | AstKind::ExportNamedDeclaration(_)
                | AstKind::ExportAllDeclaration(_)
        ) {
            return;
        }
        let (text, offset) = match node.kind() {
            AstKind::StringLiteral(lit) => (lit.value.as_str().to_string(), lit.span.start as usize),
            AstKind::TemplateLiteral(tpl) => {
                let s: String = tpl
                    .quasis
                    .iter()
                    .map(|q| q.value.raw.as_str())
                    .collect::<Vec<_>>()
                    .join("");
                (s, tpl.span.start as usize)
            }
            _ => return,
        };
        let Some(verb) = super::verb_url_match::contains_verb_url(&text) else {
            return;
        };
        let (line, column) = byte_offset_to_line_col(ctx.source, offset);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "REST URL contains the verb '{verb}' — use HTTP semantics instead. \
                 `POST /api/orders` creates, `GET /api/orders/:id` reads, \
                 `PATCH /api/orders/:id` updates, `DELETE /api/orders/:id` removes."
            ),
            severity: Severity::Warning,
            span: None,
        });
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn ignores_import_and_export_source_paths() {
        // Module import/export paths are file paths, not REST URLs (issue #1103).
        let src = "\
import { cancelScheduledNotification } from \"./api/cancelScheduledNotification.js\";
import { createOrUpdateInstallation } from \"./api/createOrUpdateInstallation.js\";
import { getInstallation } from \"./api/getInstallation.js\";
export { getFeedbackContainerUrl } from \"./api/getFeedbackContainerUrl.js\";
export * from \"./api/listRegistrations.js\";";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_verb_in_actual_url_literal() {
        assert_eq!(run("fetch('/api/createOrder');").len(), 1);
    }
}
