//! regex-no-invisible-character TypeScript / JavaScript / TSX backend.
//!
//! Flags invisible Unicode codepoints (zero-width spaces, bidi marks,
//! variation selectors, soft hyphen, BOM, etc.) that appear literally
//! inside the tree-sitter `regex` node's pattern. AST-only detection
//! eliminates FPs from comments and string literals that happen to
//! contain invisible characters.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

fn is_invisible_char(c: char) -> bool {
    matches!(c,
        '\u{00AD}'         // soft hyphen
        | '\u{034F}'       // combining grapheme joiner
        | '\u{061C}'       // arabic letter mark
        | '\u{115F}'       // hangul choseong filler
        | '\u{1160}'       // hangul jungseong filler
        | '\u{17B4}'       // khmer vowel inherent aq
        | '\u{17B5}'       // khmer vowel inherent aa
        | '\u{180E}'       // mongolian vowel separator
        | '\u{2000}'..='\u{200F}' // various spaces + zero-width + directional marks
        | '\u{202A}'..='\u{202E}' // bidi embedding / override
        | '\u{2060}'..='\u{2064}' // word joiner, invisible times/separator/plus
        | '\u{2066}'..='\u{206F}' // bidi isolates + deprecated formatting
        | '\u{FE00}'..='\u{FE0F}' // variation selectors
        | '\u{FEFF}'       // BOM / zero-width no-break space
        | '\u{FFF9}'..='\u{FFFB}' // interlinear annotations
    )
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some((pattern, _flags)) = pattern_and_flags(&node, source) else { return };
    if !pattern.chars().any(is_invisible_char) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-invisible-character",
        "Invisible Unicode character in regex \u{2014} use an explicit `\\u{...}` escape instead.".into(),
        Severity::Warning,
    ));
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
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_zero_width_space() {
        assert_eq!(run_on("const re = /foo\u{200B}bar/;").len(), 1);
    }

    #[test]
    fn flags_soft_hyphen() {
        assert_eq!(run_on("const re = /test\u{00AD}word/;").len(), 1);
    }

    #[test]
    fn allows_clean_regex() {
        assert!(run_on("const re = /foo/;").is_empty());
    }

    #[test]
    fn allows_non_regex_line() {
        assert!(run_on("const x = 42;").is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_class_string() {
        assert!(run_on(r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#).is_empty());
    }

    #[test]
    fn ignores_url_string() {
        assert!(run_on(r#"const u = "http://a/b/c";"#).is_empty());
    }

    #[test]
    fn ignores_import_path() {
        assert!(run_on(r#"import X from "@scope/pkg/sub";"#).is_empty());
    }
}
