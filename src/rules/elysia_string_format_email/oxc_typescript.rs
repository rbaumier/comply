//! elysia-string-format-email oxc backend — flag schema fields named after a
//! known string format that use bare `t.String()`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

const PATTERNS: &[&str] = &["email:t.String()", "url:t.String()", "uri:t.String()"];

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !ctx.project.has_framework("elysia") {
            return Vec::new();
        }

        let norm: String = ctx.source.chars().filter(|c| !c.is_whitespace()).collect();
        let mut hit = false;
        for pat in PATTERNS {
            if norm.contains(pat) {
                hit = true;
                break;
            }
        }
        if !hit {
            return Vec::new();
        }

        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "Field named after a known format uses bare `t.String()` — add `{ format: 'email' }` (or `'uri'`).".into(),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_email_field() {
        let src = "import { t } from 'elysia';\nconst s = t.Object({ email: t.String() });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_url_field() {
        let src = "import { t } from 'elysia';\nconst s = t.Object({ url: t.String() });";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_email_with_format() {
        let src = "import { t } from 'elysia';\nconst s = t.Object({ email: t.String({ format: 'email' }) });";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "const s = t.Object({ email: t.String() });";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }
}
