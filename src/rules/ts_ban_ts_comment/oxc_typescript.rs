//! ts-ban-ts-comment OXC backend — flag @ts-ignore, @ts-nocheck, and bare
//! @ts-expect-error via semantic comments. Test files are exempt from the
//! `@ts-expect-error` description requirement: test dirs/suffixes (`.test.`,
//! `.spec.`, `tests/`, `__tests__/`, …) plus tsd/dtslint type-test files
//! (`test-d/` or `dtslint/` directories, `*.test-d.{ts,tsx}`,
//! `*.types-test.{ts,tsx}`). There a bare directive is a type-level assertion
//! documented by the enclosing `it()`/`describe()` name. `@ts-ignore` and
//! `@ts-nocheck` remain banned everywhere, including test files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

/// True when `path` is a tsd/dtslint type-level test file, where a bare
/// `@ts-expect-error` is the conventional assertion that an expression is a
/// type error. Conventions: files under a `test-d/` (tsd) or `dtslint/`
/// directory, or named `*.test-d.{ts,tsx}` / `*.types-test.{ts,tsx}`.
fn is_type_test_file(path: &std::path::Path) -> bool {
    if path
        .components()
        .any(|c| c.as_os_str() == "test-d" || c.as_os_str() == "dtslint")
    {
        return true;
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            name.ends_with(".test-d.ts")
                || name.ends_with(".test-d.tsx")
                || name.ends_with(".types-test.ts")
                || name.ends_with(".types-test.tsx")
        })
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@ts-"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // The `@ts-expect-error` description requirement is waived in test
        // files: there a bare directive is a type-level assertion documented
        // by the enclosing `it()`/`describe()`. tsd/dtslint type-test files
        // (incl. `*.test-d.ts` / `*.types-test.ts` suffixes not covered by
        // `in_test_dir`) keep their existing exemption. `@ts-ignore` and
        // `@ts-nocheck` remain banned everywhere — this gate touches only the
        // `@ts-expect-error` branch below.
        let expect_error_exempt =
            is_type_test_file(ctx.path) || ctx.file.path_segments.in_test_dir;

        for comment in semantic.comments() {
            // OXC comment spans INCLUDE the `//` or `/* */` markers
            let text = &ctx.source[comment.span.start as usize..comment.span.end as usize];
            let stripped = text.trim_start_matches('/').trim_start_matches('*').trim();

            if let Some(_rest) = stripped.strip_prefix("@ts-ignore") {
                let (line, column) = byte_offset_to_line_col(ctx.source, comment.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Use `@ts-expect-error` instead of `@ts-ignore`, as `@ts-ignore` will do nothing if the following line is error-free.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            } else if let Some(_rest) = stripped.strip_prefix("@ts-nocheck") {
                let (line, column) = byte_offset_to_line_col(ctx.source, comment.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "Do not use `@ts-nocheck` because it alters compilation errors.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            } else if let Some(rest) = stripped.strip_prefix("@ts-expect-error") {
                let description = rest.trim();
                if !expect_error_exempt && (description.is_empty() || description.len() < 3) {
                    let (line, column) = byte_offset_to_line_col(ctx.source, comment.span.start as usize);
                    diagnostics.push(Diagnostic {
                        path: Arc::clone(&ctx.path_arc),
                        line,
                        column,
                        rule_id: super::META.id.into(),
                        message: "Include a description after `@ts-expect-error` to explain why it is necessary (at least 3 characters).".into(),
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

    // `run_rule_gated` builds the FileCtx from the path (so
    // `path_segments.in_test_dir` matches production), unlike `run_rule`
    // which uses a static default FileCtx.
    fn run_at(src: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, src, path)
    }

    #[test]
    fn ignores_bare_expect_error_in_test_d_directory() {
        let src = "// Missing generic parameter\n// @ts-expect-error\ntype A = IsAny;\n\ntype B = OnlyAny<any>;\n// @ts-expect-error\ntype C = OnlyAny<string>;\n";
        assert!(run_at(src, "test-d/is-any.ts").is_empty());
    }

    #[test]
    fn ignores_bare_expect_error_in_test_d_suffixed_file() {
        let src = "// @ts-expect-error\ntype A = IsAny;\n";
        assert!(run_at(src, "and.test-d.ts").is_empty());
    }

    #[test]
    fn flags_bare_expect_error_in_regular_src() {
        let src = "// @ts-expect-error\nconst x: number = 'a';\n";
        assert_eq!(run_at(src, "src/foo.ts").len(), 1);
    }

    #[test]
    fn flags_ts_ignore_in_test_d_directory() {
        let src = "// @ts-ignore\ntype A = IsAny;\n";
        assert_eq!(run_at(src, "test-d/x.ts").len(), 1);
    }

    #[test]
    fn exempts_bare_expect_error_in_test_file_issue_1033() {
        // ts-pattern tests/*.test.ts: bare @ts-expect-error is a type assertion.
        let src = "it('x', () => {\n  // @ts-expect-error\n  const r = bad();\n});\n";
        assert!(run_at(src, "tests/record.test.ts").is_empty(), "{:?}", run_at(src, "tests/record.test.ts"));
    }

    #[test]
    fn exempts_bare_expect_error_in_dot_test_suffixed_file() {
        let src = "// @ts-expect-error\nconst x: number = 'a';\n";
        assert!(run_at(src, "src/record.test.ts").is_empty());
    }

    #[test]
    fn still_flags_ts_ignore_in_test_file_issue_1033() {
        // @ts-ignore stays banned everywhere, including test files.
        let src = "// @ts-ignore\nconst x = bad;\n";
        assert_eq!(run_at(src, "tests/record.test.ts").len(), 1);
    }

    #[test]
    fn still_flags_ts_nocheck_in_test_file() {
        let src = "// @ts-nocheck\nconst x = bad;\n";
        assert_eq!(run_at(src, "tests/record.test.ts").len(), 1);
    }
}
