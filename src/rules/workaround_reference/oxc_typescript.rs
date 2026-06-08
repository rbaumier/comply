use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["workaround", "hack", "compat", "Workaround", "Hack", "Compat", "HACK"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            if end > ctx.source.len() {
                continue;
            }
            let text = &ctx.source[start..end];

            if !super::has_keyword(text) {
                continue;
            }
            if super::has_reference(text) {
                continue;
            }

            let (line, _) = byte_offset_to_line_col(ctx.source, start);
            let row = line.saturating_sub(1);
            let lookahead = (row + 1..=(row + 2).min(lines.len().saturating_sub(1)))
                .any(|i| super::has_reference(lines[i]));
            if lookahead {
                continue;
            }

            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column: 1,
                rule_id: super::META.id.into(),
                message: "Workaround/hack/compat comment without an issue reference — \
                          add a link or ticket number."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use crate::rules::workaround_reference::oxc_typescript::Check;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_workaround_without_ref() {
        let diags = run("// Workaround for fish\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_workaround_with_issue_ref() {
        assert!(run("// Workaround for a fish bug (see #739, #279)\nconst x = 1;").is_empty());
    }


    #[test]
    fn allows_workaround_with_url() {
        assert!(
            run("// Workaround for https://github.com/org/repo/issues/1\nconst x = 1;")
                .is_empty()
        );
    }


    #[test]
    fn flags_hack_without_ref() {
        let diags = run("// hack to fix rendering\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_jira_ref() {
        assert!(run("// Workaround for PROJ-123\nconst x = 1;").is_empty());
    }


    #[test]
    fn no_fp_on_compatible_type_description() {
        // "structurally compatible with RelationalWhere<T>" — pure type-system term, not a workaround
        let src = r#"
/**
 * The returned shape is structurally compatible with `RelationalWhere<TTable>`
 * for every table that declares a `deactivatedAt` column.
 */
const x = 1;
"#;
        assert!(run(src).is_empty());
    }


    #[test]
    fn no_fp_on_incompatible() {
        assert!(run("// These APIs are incompatible with each other\nconst x = 1;").is_empty());
    }


    #[test]
    fn no_fp_on_compatibility() {
        // Regression for issue #543: "compat" inside "compatibility" is not a marker.
        assert!(run("// improves backward compatibility of the API\nconst x = 1;").is_empty());
    }


    #[test]
    fn no_fp_on_hackathon() {
        assert!(run("// built during the 2024 hackathon\nconst x = 1;").is_empty());
    }


    #[test]
    fn flags_compat_layer() {
        let diags = run("// compat layer for old browsers\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_compat_fix() {
        let diags = run("// compat fix for Safari\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }
}
