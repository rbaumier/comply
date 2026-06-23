use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

const TEST_MARKERS: &[&str] = &[".test.", ".spec.", "__tests__", "_test.", ".e2e."];

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    TEST_MARKERS.iter().any(|m| s.contains(m))
}

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
        if is_test_file(ctx.path) {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();

        for comment in semantic.comments() {
            let start = comment.span.start as usize;
            let end = comment.span.end as usize;
            if end > ctx.source.len() {
                continue;
            }

            // `@deprecated` is a JSDoc tag, which only lives in block comments.
            // Use the comment kind rather than slicing back to the `/*` prefix:
            // a preceding multi-byte codepoint would make `start - 2` land off a
            // char boundary and panic.
            if !comment.is_block() {
                continue;
            }

            let Some(text) = ctx.source.get(start..end) else {
                continue;
            };
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_deprecated_without_message() {
        let d = run_on("/**\n * @deprecated\n */\nexport function f() {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "deprecation-without-alternative");
    }

    #[test]
    fn allows_deprecated_with_message() {
        let d = run_on("/**\n * @deprecated use g() instead\n */\nexport function f() {}");
        assert!(d.is_empty());
    }

    #[test]
    fn no_panic_on_box_drawing_chars_before_comment() {
        // #4695: box-drawing chars (U+2500, 3 bytes) preceding a `@deprecated`
        // comment made `start - 2` land inside a multi-byte codepoint.
        let src = "// ── section ──────────\n/**\n * @deprecated\n */\nexport const x = 1;";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn no_panic_on_emoji_and_cjk() {
        let src = "// 🚀 配置 separator\n/**\n * @deprecated\n */\nexport const y = 2;";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn no_panic_on_multibyte_inside_deprecated_comment() {
        let src = "/**\n * ── 配置 🚀\n * @deprecated\n */\nexport const z = 3;";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }
}
