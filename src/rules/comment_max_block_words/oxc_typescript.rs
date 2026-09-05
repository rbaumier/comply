use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use crate::rules::comment_blocks;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let max = ctx.config.threshold(super::META.id, "max", ctx.lang);

        let comments = comment_blocks::from_oxc(semantic, ctx.source);

        super::flagged_blocks(comments, ctx.source, max)
            .into_iter()
            .map(|flag| Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: flag.line,
                column: flag.column,
                rule_id: super::META.id.into(),
                message: super::message(flag.words, max),
                severity: Severity::Error,
                span: None,
            })
            .collect()
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_long_line_comment_block() {
        let src = "\
// this is a long implementation note that keeps explaining the rationale in
// exhaustive detail across several full lines and easily runs past the fifty
// word budget because it just keeps going and going and going and going and
// going and never stops adding one more clause that could have lived in a
// dedicated doc comment or a shorter summary somewhere far more scannable here
fn_placeholder();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_short_block() {
        assert!(run("// short note\n// second short line\nconst x = 1;").is_empty());
    }

    // The same column of trailing field labels the Rust backend keeps apart:
    // both backends read blocks through the shared merge.
    #[test]
    fn column_aligned_trailing_field_labels_are_not_one_block() {
        let src = "\
export const maxp = [
  0x00010000, // version
  0x00000001, // number of glyphs
  0x00000000, // maximum points in a non-composite glyph
  0x00000000, // maximum contours in a non-composite glyph
  0x00000000, // maximum points in a composite glyph
  0x00000000, // maximum contours in a composite glyph
  0x00000002, // maximum zones used for twilight and glyph space
  0x00000000, // maximum twilight points used in zone zero
  0x00000000, // number of storage area locations available
  0x00000000, // maximum function definitions in the font program
];";
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    #[test]
    fn jsdoc_block_counts_too() {
        let src = r#"/**
 * This JSDoc block explains the loader integration pattern in thorough detail,
 * covering the relationship between the preload mechanism and the form dialog
 * lifecycle across many rendering phases and async boundary contexts here and
 * there and everywhere well past any reasonable fifty word inline note budget,
 * which is exactly the kind of wall of prose the budget exists to break apart.
 */
export function f() {}"#;
        assert_eq!(run(src).len(), 1);
    }
}
