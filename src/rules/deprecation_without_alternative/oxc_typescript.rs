use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@deprecated"])
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
            if end > ctx.source.len() {
                continue;
            }

            let prefix_start = start.saturating_sub(2);
            let with_prefix = &ctx.source[prefix_start..end];
            if !with_prefix.starts_with("/*") {
                continue;
            }

            let text = &ctx.source[start..end];
            let Some(dep_pos) = text.find("@deprecated") else {
                continue;
            };

            let after = text[dep_pos + "@deprecated".len()..].trim_start();
            if !after.is_empty() && !after.starts_with('*') && !after.starts_with('\n') {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(ctx.source, start + dep_pos);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`@deprecated` without a migration message — \
                          add text after the tag explaining what to use instead."
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
    use crate::rules::deprecation_without_alternative::oxc_typescript::Check;



    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_bare_deprecated_jsdoc() {
        let diags = run("/** @deprecated */\nfunction old() {}");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn flags_deprecated_on_own_line() {
        let diags = run("/**\n * @deprecated\n */\nfunction old() {}");
        assert_eq!(diags.len(), 1);
    }


    #[test]
    fn allows_deprecated_with_message() {
        assert!(run("/** @deprecated Use newFn instead */\nfunction old() {}").is_empty());
    }


    #[test]
    fn allows_deprecated_with_message_multiline() {
        assert!(run("/**\n * @deprecated Use newFn instead.\n */\nfunction old() {}").is_empty());
    }


    #[test]
    fn ignores_non_jsdoc() {
        assert!(run("const x = 1;").is_empty());
    }


    #[test]
    fn ignores_line_comment() {
        assert!(run("// @deprecated\nfunction old() {}").is_empty());
    }
}
