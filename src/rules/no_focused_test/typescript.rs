//! no-focused-test backend — flag `.only` on test/it/describe.
//!
//! Why: a single `it.only` committed to main silently disables every
//! other test in the suite. CI runs, reports green, and regressions slip
//! through because only the one focused test actually ran. The cost of
//! committing a focused test is catastrophically asymmetric — catch it
//! at the linter.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some(m) = crate::rules::test_methods::match_test_member_call(node, source) else {
        return;
    };
    if m.method != "only" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-focused-test".into(),
        message: format!(
            "`{base}.only` silently disables every other test in the suite \
             when committed. Remove `.only` before pushing.",
            base = m.base,
        ),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_test_only() {
        assert_eq!(run_on("test.only('x', () => {});").len(), 1);
    }

    #[test]
    fn flags_it_only() {
        assert_eq!(run_on("it.only('x', () => {});").len(), 1);
    }

    #[test]
    fn flags_describe_only() {
        assert_eq!(run_on("describe.only('x', () => {});").len(), 1);
    }

    #[test]
    fn allows_regular_test() {
        assert!(run_on("test('x', () => {});").is_empty());
    }
}
