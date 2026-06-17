//! OXC backend for no-useless-switch-case.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::SwitchStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::SwitchStatement(switch) = node.kind() else {
            return;
        };

        let cases = &switch.cases;
        if cases.len() < 2 {
            return;
        }

        // Last case must be `default` (test is None).
        let last = &cases[cases.len() - 1];
        if last.test.is_some() {
            return;
        }
        let default_start = last.span.start as usize;

        // Walk backwards from the case just before default and flag empty cases.
        let mut i = cases.len() - 2;
        loop {
            let case = &cases[i];
            // Must be a `case X:` (not default).
            if case.test.is_none() {
                break;
            }

            // A case is "empty" if it has no consequent statements
            // (or only empty statements / comments aren't represented in OXC AST).
            let is_empty = case.consequent.is_empty()
                || case.consequent.iter().all(|s| matches!(s, Statement::EmptyStatement(_)));

            if !is_empty {
                break;
            }

            // Honor the fallthrough-comment convention (like ESLint
            // `no-fallthrough`'s `commentPattern`): if the trivia between this
            // empty case and `default` carries an explicit fallthrough-intent
            // comment, the author deliberately enumerated these labels — don't
            // flag. The comment sits just before `default`, so it lies inside
            // each stacked empty case's case→default region; scanning that
            // bounded slice per flagged case covers the whole chain.
            if has_fallthrough_comment(&ctx.source[case.span.end as usize..default_start]) {
                break;
            }

            let (line, column) =
                byte_offset_to_line_col(ctx.source, case.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Useless case in switch statement — it falls through \
                          to `default` with no own code."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });

            if i == 0 {
                break;
            }
            i -= 1;
        }
    }
}

/// Whether `trivia` (the source between an empty case and `default`) carries an
/// explicit fallthrough-intent comment. Mirrors the common forms recognized by
/// ESLint `no-fallthrough`'s `commentPattern`: `fallthrough` (one word) and the
/// spaced/hyphenated `fall(s) through` variants, matched case-insensitively.
fn has_fallthrough_comment(trivia: &str) -> bool {
    let lower = trivia.to_ascii_lowercase();
    lower.contains("fallthrough")
        || lower.contains("falls through")
        || lower.contains("fall through")
        || lower.contains("falls-through")
        || lower.contains("fall-through")
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

    // ---- flags useless cases (no fallthrough comment) ----

    #[test]
    fn flags_single_empty_case_before_default() {
        let src = r#"
switch (x) {
    case 1:
    default:
        break;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_multiple_empty_cases_before_default() {
        let src = r#"
switch (x) {
    case 1:
    case 2:
    case 3:
    default:
        break;
}
"#;
        assert_eq!(run_on(src).len(), 3);
    }

    // ---- allows correct usage ----

    #[test]
    fn allows_case_with_body() {
        let src = r#"
switch (x) {
    case 1:
        console.log('one');
        break;
    default:
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_no_default() {
        let src = r#"
switch (x) {
    case 1:
    case 2:
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_fallthrough_to_case_not_default() {
        let src = r#"
switch (x) {
    case 1:
    case 2:
        console.log('1 or 2');
        break;
    default:
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }

    // ---- honors the fallthrough-intent comment convention (#3959) ----

    // Regression for #3959: the typescript-eslint enum-exhaustiveness shape —
    // stacked empty cases annotated `// intentional fallthrough` before an
    // empty `default`. ESLint `no-fallthrough` silences exactly this.
    #[test]
    fn allows_stacked_cases_with_intentional_fallthrough_comment() {
        let src = r#"
switch (token.kind) {
    case SyntaxKind.Identifier:
    case SyntaxKind.ConstructorKeyword:
    case SyntaxKind.GetKeyword:
    case SyntaxKind.SetKeyword:
    // intentional fallthrough
    default:
}
"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_single_case_with_falls_through_comment() {
        let src = r#"
switch (x) {
    case 1:
    // falls through
    default:
        break;
}
"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    #[test]
    fn allows_single_case_with_one_word_fallthrough_comment() {
        let src = r#"
switch (x) {
    case 1:
    // fallthrough
    default:
        break;
}
"#;
        assert!(run_on(src).is_empty(), "{:?}", run_on(src));
    }

    // ---- does NOT over-suppress ----

    #[test]
    fn flags_stacked_cases_without_comment() {
        let src = r#"
switch (x) {
    case 1:
    case 2:
    default:
        break;
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn unrelated_comment_does_not_suppress() {
        let src = r#"
switch (x) {
    case 1:
    // TODO: handle this later
    default:
        break;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }
}
