//! no-test-prefixes backend — flag Jasmine-style prefixes on test helpers.
//!
//! `ftest` / `fdescribe` / `fit` focus a test; `xtest` / `xdescribe` / `xit`
//! skip it. Both mutate CI behavior silently and are trivially confused with
//! real helpers during review. The explicit `.only` / `.skip` modifiers
//! surface intent where a reader expects to find it.

use crate::diagnostic::{Diagnostic, Severity};

const FLAGGED: &[&str] = &["ftest", "fdescribe", "fit", "xtest", "xdescribe", "xit"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !FLAGGED.contains(&name) {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-test-prefixes".into(),
        message: format!(
            "`{name}` uses a Jasmine-style f/x prefix to focus or skip a test. \
             Use .only or .skip modifiers instead."
        ),
        severity: Severity::Warning,
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
    fn flags_ftest() {
        assert_eq!(run_on("ftest('x', () => {});").len(), 1);
    }

    #[test]
    fn flags_fdescribe() {
        assert_eq!(run_on("fdescribe('x', () => {});").len(), 1);
    }

    #[test]
    fn flags_fit() {
        assert_eq!(run_on("fit('x', () => {});").len(), 1);
    }

    #[test]
    fn flags_xtest() {
        assert_eq!(run_on("xtest('x', () => {});").len(), 1);
    }

    #[test]
    fn flags_xdescribe() {
        assert_eq!(run_on("xdescribe('x', () => {});").len(), 1);
    }

    #[test]
    fn flags_xit() {
        assert_eq!(run_on("xit('x', () => {});").len(), 1);
    }

    #[test]
    fn allows_regular_test() {
        assert!(run_on("test('x', () => {});").is_empty());
    }

    #[test]
    fn allows_test_only() {
        assert!(run_on("test.only('x', () => {});").is_empty());
    }

    #[test]
    fn allows_describe_skip() {
        assert!(run_on("describe.skip('x', () => {});").is_empty());
    }

    #[test]
    fn allows_similarly_named_identifier() {
        assert!(run_on("fitness('x');").is_empty());
    }
}
