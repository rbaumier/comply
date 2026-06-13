//! OxcCheck backend — flag `test("... and ...", ...)` / `it("... and ...", ...)`
//! names that combine multiple behaviors.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let path = ctx.path.to_string_lossy();
        if !path.contains(".test.") && !path.contains(".spec.") {
            return;
        }
        let AstKind::CallExpression(call) = node.kind() else { return };
        let callee_name = match &call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if callee_name != "test" && callee_name != "it" {
            return;
        }
        let Some(first_arg) = call.arguments.first() else { return };
        let (unquoted, span_start) = match first_arg {
            Argument::StringLiteral(s) => (s.value.as_str(), s.span.start as usize),
            Argument::TemplateLiteral(t) => {
                // Only check simple template literals (no expressions)
                if !t.expressions.is_empty() || t.quasis.len() != 1 {
                    return;
                }
                (t.quasis[0].value.raw.as_str(), t.span.start as usize)
            }
            _ => return,
        };
        if !joins_two_behaviors(unquoted) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, span_start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Test name {unquoted:?} contains \" and \" — split into two focused tests."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// Whether a test title joins two independent behaviors with " and ".
///
/// Fires only on the parallel-predicate shape `<verb> <complement> and <verb>
/// <complement>` — two finite verbs in third-person-singular present (`-s`)
/// form, each leading its own clause with a complement. That is the genuine
/// "this test asserts two things" signal worth splitting.
///
/// A bare `" and "` is otherwise a grammatical particle inside a single
/// behavior, so these are NOT flagged:
/// - noun/condition conjunctions (`for null and undefined`, `markup and styles`),
/// - condition or modifier clauses led by `when`/`with`/`for` rather than a verb,
/// - compound verbs sharing one object (`should filter and render stories`,
///   `reads and writes the stream`),
/// - a trailing verb with no complement (`extracts imports … and dedupes`).
fn joins_two_behaviors(title: &str) -> bool {
    // The first behavior must be verb-led: a third-person-singular present verb
    // followed by at least one complement word before any " and ".
    let mut words = title.split_whitespace();
    let Some(leading) = words.next() else { return false };
    if !is_third_person_verb(leading) {
        return false;
    }

    // Walk the remaining words; the predicate after a " and " must itself start
    // with a third-person verb and carry a complement (a following word).
    let rest: Vec<&str> = words.collect();
    let mut saw_complement_before_and = false;
    let mut idx = 0;
    while idx < rest.len() {
        if rest[idx] == "and" {
            if saw_complement_before_and
                && let Some(next) = rest.get(idx + 1)
                && is_third_person_verb(next)
                && rest.get(idx + 2).is_some()
            {
                return true;
            }
            // This conjunction isn't a second predicate; keep scanning.
            saw_complement_before_and = false;
        } else {
            saw_complement_before_and = true;
        }
        idx += 1;
    }
    false
}

/// Whether a word reads as a third-person-singular present verb (`sends`,
/// `creates`, `returns`): lowercase ASCII letters ending in `s`, excluding
/// auxiliaries and common non-verb `-s` words that would misfire.
fn is_third_person_verb(word: &str) -> bool {
    // Strip a trailing comma from clause-final words like "empty,".
    let word = word.trim_end_matches(',');
    if word.len() < 3 || !word.bytes().all(|b| b.is_ascii_lowercase()) {
        return false;
    }
    if !word.ends_with('s') || word.ends_with("ss") {
        return false;
    }
    !matches!(
        word,
        "is" | "was" | "has" | "as" | "its" | "this" | "thus" | "plus" | "less"
    )
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
        crate::rules::test_helpers::run_rule(&Check, s, "foo.test.ts")
    }

    #[test]
    fn flags_two_verb_led_behaviors() {
        assert_eq!(
            run("test('validates email and sends confirmation', () => {})").len(),
            1
        );
        assert_eq!(
            run("it('creates a user and sends an email', () => {})").len(),
            1
        );
    }

    #[test]
    fn allows_single_behavior() {
        assert!(run("test('validates email format', () => {})").is_empty());
    }

    // Regression — issue #2062: " and " describing one compound condition/input.
    #[test]
    fn allows_condition_clause() {
        assert!(
            run(
                "it('when host is 0.0.0.0 and allowedHosts is empty, permits all hosts', () => {})"
            )
            .is_empty()
        );
    }

    #[test]
    fn allows_noun_phrase_modifier() {
        assert!(
            run("it('returns [] for a Svelte file with only markup and styles', () => {})")
                .is_empty()
        );
    }

    #[test]
    fn allows_trailing_verb_without_complement() {
        assert!(
            run("it('extracts imports from BOTH instance and module scripts and dedupes', () => {})")
                .is_empty()
        );
    }

    #[test]
    fn allows_compound_verb_sharing_one_object() {
        assert!(run("it('should filter and render composed stories', () => {})").is_empty());
        assert!(run("test('reads and writes the stream', () => {})").is_empty());
    }

    #[test]
    fn allows_noun_enumeration_after_preposition() {
        assert!(run("test('returns 0 for null and undefined', () => {})").is_empty());
    }
}
