//! regex-no-single-char-class OXC backend — visits RegExpLiteral nodes only.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

/// Each hit is the class's byte offset within `pattern` paired with its
/// `[x]` snippet. The offset lets the caller anchor the diagnostic on the
/// class itself rather than on the enclosing regex literal.
fn find_single_char_classes(pattern: &str) -> Vec<(usize, String)> {
    let mut hits = Vec::new();
    let bytes = pattern.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 2 < len {
        if bytes[i] == b'['
            && bytes[i + 1] != b'^'
            && bytes[i + 1] != b'\\'
            && bytes[i + 1] != b']'
            && bytes[i + 2] == b']'
        {
            let backslashes = bytes[..i].iter().rev().take_while(|&&b| b == b'\\').count();
            if backslashes % 2 == 0 {
                hits.push((i, pattern[i..i + 3].to_string()));
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    hits
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::RegExpLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::RegExpLiteral(re) = node.kind() else { return };
        let pattern = re.regex.pattern.text.as_str();
        // `pattern.text` is the verbatim source slice between the slashes, which
        // the parser anchors at `re.span.start + 1` (the `+ 1` skips the opening
        // `/`). So an in-pattern byte offset maps straight into the source, and
        // each class is reported at its own column instead of the literal start.
        let pattern_start = re.span.start as usize + 1;
        for (offset, snippet) in find_single_char_classes(pattern) {
            let (line, column) = byte_offset_to_line_col(ctx.source, pattern_start + offset);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Unnecessary single-character class `{}` \u{2014} use the character directly (or escape it).",
                    snippet,
                ),
                severity: Severity::Error,
                span: None,
            });
        }
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    /// 1-based column of the regex literal's opening `/` — the position no
    /// per-class diagnostic should collapse onto.
    fn literal_start_column(source: &str) -> usize {
        source.find('/').expect("test source has a regex literal") + 1
    }

    #[test]
    fn reports_each_class_at_its_own_distinct_column() {
        // Four `[']` single-char classes in one literal.
        let src = "const re = /['] ['] ['] [']/;";
        let d = run_on(src);
        assert_eq!(d.len(), 4, "four single-char classes → four diagnostics");
        assert!(d.iter().all(|x| x.line == 1));

        let columns: BTreeSet<usize> = d.iter().map(|x| x.column).collect();
        assert_eq!(columns.len(), 4, "each diagnostic at its own column, not all pinned to the literal start");

        for x in &d {
            assert_eq!(src.as_bytes()[x.column - 1], b'[', "column must land on the class's opening `[`");
            assert!(x.message.contains("`[']`"));
        }

        let start = literal_start_column(src);
        assert!(d.iter().all(|x| x.column != start), "none should sit at the regex literal start");
    }

    #[test]
    fn single_class_not_at_start_reports_at_its_own_column() {
        let src = "const re = /abc[x]/;";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, 1);
        assert_eq!(src.as_bytes()[d[0].column - 1], b'[', "column must land on the `[`");
        assert_ne!(d[0].column, literal_start_column(src), "must not be pinned to the regex literal start");
    }

    #[test]
    fn class_at_start_reports_once() {
        let src = "const re = /[x]/;";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(src.as_bytes()[d[0].column - 1], b'[');
        assert!(d[0].message.contains("`[x]`"));
    }

    #[test]
    fn ignores_multi_char_and_negated_classes() {
        assert!(run_on("const re = /[xy]/;").is_empty());
        assert!(run_on("const re = /[^x]/;").is_empty());
    }
}
