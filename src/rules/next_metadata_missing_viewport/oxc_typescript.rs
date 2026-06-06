use crate::diagnostic::{Diagnostic, Severity};
use crate::project::Framework;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

fn source_has_viewport_export(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "export const viewport")
        || crate::oxc_helpers::source_contains(source, "export function generateViewport")
        || crate::oxc_helpers::source_contains(source, "export async function generateViewport")
        || crate::oxc_helpers::source_contains(source, "export let viewport")
        || crate::oxc_helpers::source_contains(source, "export var viewport")
}

fn source_has_metadata_export(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "export const metadata")
        || crate::oxc_helpers::source_contains(source, "export function generateMetadata")
        || crate::oxc_helpers::source_contains(source, "export async function generateMetadata")
        || crate::oxc_helpers::source_contains(source, "export let metadata")
        || crate::oxc_helpers::source_contains(source, "export var metadata")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [crate::rules::backend::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.project.framework != Framework::NextJs {
            return Vec::new();
        }
        if !source_has_metadata_export(ctx.source) {
            return Vec::new();
        }
        if source_has_viewport_export(ctx.source) {
            return Vec::new();
        }
        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Page exports `metadata` but not `viewport` \u{2014} add a `viewport` export with `width: 'device-width'`.".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}
