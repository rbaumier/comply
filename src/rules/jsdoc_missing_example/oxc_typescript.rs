//! jsdoc-missing-example OxcCheck backend — every JSDoc on an exported function
//! must contain an `@example` tag, unless the block is tagged `@deprecated`
//! (a deprecated function shouldn't document how to call it).

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["/**"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportNamedDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let public_patterns = ctx
            .config
            .string_list("jsdoc-missing-example", "public_patterns", ctx.lang);
        if !public_patterns.is_empty() && !path_matches_any(ctx.path, &public_patterns) {
            return;
        }

        let AstKind::ExportNamedDeclaration(export) = node.kind() else {
            return;
        };

        // Only care about exported function declarations.
        let Some(decl) = &export.declaration else {
            return;
        };
        let oxc_ast::ast::Declaration::FunctionDeclaration(func) = decl else {
            return;
        };

        let export_start = export.span.start;

        // Find a JSDoc comment preceding this export.
        let Some(jsdoc_text) = find_jsdoc_above(semantic, ctx.source, export_start) else {
            // No JSDoc — that's jsdoc-on-exported's job, not ours.
            return;
        };

        if jsdoc_text.contains("@example") {
            return;
        }

        // A `@deprecated` block tells callers to stop using the function;
        // requiring an example of how to call it would contradict that.
        if jsdoc_text.contains("@deprecated") {
            return;
        }

        let name = func
            .id
            .as_ref()
            .map(|id| id.name.as_str())
            .unwrap_or("<anonymous>");

        let (line, column) = byte_offset_to_line_col(ctx.source, export_start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "jsdoc-missing-example".into(),
            message: format!(
                "JSDoc on `{name}` is missing `@example`. Add a real call \
                 and its return value — examples are the fastest way for \
                 callers to understand the API."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True if `path` matches at least one glob pattern from `patterns`.
fn path_matches_any(path: &std::path::Path, patterns: &[String]) -> bool {
    let path_str = path.to_string_lossy();
    patterns.iter().any(|pat| {
        globset::Glob::new(pat)
            .ok()
            .map(|g| g.compile_matcher().is_match(path_str.as_ref()))
            .unwrap_or(false)
    })
}

/// Find a JSDoc comment (`/** ... */`) immediately above a given byte position.
fn find_jsdoc_above<'a>(
    semantic: &'a oxc_semantic::Semantic<'a>,
    source: &'a str,
    export_start: u32,
) -> Option<&'a str> {
    // Scan comments for the closest `/** ... */` that ends before export_start.
    let mut best: Option<(u32, &str)> = None;
    for comment in semantic.comments() {
        // comment span includes the markers
        let c_start = comment.span.start;
        let c_end = comment.span.end;
        if c_end > export_start {
            continue;
        }
        let text = &source[c_start as usize..c_end as usize];
        if !text.starts_with("/**") {
            continue;
        }
        // Keep the closest one before the export.
        if best.is_none_or(|(prev_end, _)| c_end > prev_end) {
            best = Some((c_end, text));
        }
    }

    let (end, text) = best?;
    // Only match if the comment is directly adjacent (only whitespace between).
    let between = &source[end as usize..export_start as usize];
    if between.trim().is_empty() {
        Some(text)
    } else {
        None
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
    fn flags_jsdoc_without_example() {
        let d = run_on(
            "/**\n * Adds two numbers.\n */\nexport function add(a: number, b: number): number {\n  return a + b;\n}",
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "jsdoc-missing-example");
    }

    #[test]
    fn allows_jsdoc_with_example() {
        let d = run_on(
            "/**\n * Adds two numbers.\n * @example\n *   add(1, 2) // => 3\n */\nexport function add(a: number, b: number): number {\n  return a + b;\n}",
        );
        assert!(d.is_empty());
    }

    #[test]
    fn allows_deprecated_without_example() {
        // #6938: a `@deprecated` function shouldn't be required to document an example.
        let d = run_on(
            "/**\n * @deprecated Use `defineRobotsSchema()` instead.\n */\nexport function asSeoCollection(c: number): number {\n  return c;\n}",
        );
        assert!(d.is_empty());
    }
}
