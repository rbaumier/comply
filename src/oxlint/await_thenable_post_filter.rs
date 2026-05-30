//! Post-filter for `await-thenable` false positives in test files.
//!
//! RTL's `render()` is synchronous and returns `RenderResult`, not a Promise.
//! `await`ing a non-thenable is valid JS (no-op at runtime) and idiomatic in
//! `async` test bodies alongside real awaits like `await userEvent.click()`.
//! tsgolint correctly identifies the non-thenable `await`, but in a test file
//! the pattern is intentional — suppress it. (Closes #449)

use crate::diagnostic::Diagnostic;

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "await-thenable" {
            return true;
        }
        !is_test_path(&d.path)
    });
}

fn is_test_path(path: &std::path::Path) -> bool {
    let lower = path.to_string_lossy().replace('\\', "/");
    lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.contains("/__tests__/")
        || lower.starts_with("__tests__/")
        || lower.contains("/tests/")
        || lower.contains("/test/")
        || lower.starts_with("tests/")
        || lower.starts_with("test/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;
    use std::borrow::Cow;
    use std::path::Path;
    use std::sync::Arc;

    fn diag(path: &str, rule: &'static str) -> Diagnostic {
        Diagnostic {
            path: Arc::from(Path::new(path)),
            line: 1,
            column: 1,
            rule_id: Cow::Borrowed(rule),
            message: String::new(),
            severity: Severity::Error,
            span: None,
        }
    }

    // Regression for #449: await renderWithProviders() in RTL test file must not fire.
    #[test]
    fn drops_await_thenable_in_test_file() {
        let mut diags = vec![diag(
            "src/features/product/product-row-actions.test.tsx",
            "await-thenable",
        )];
        apply(&mut diags);
        assert!(diags.is_empty(), "await-thenable in .test.tsx must be suppressed");
    }

    #[test]
    fn drops_await_thenable_in_spec_file() {
        let mut diags = vec![diag("src/utils/format.spec.ts", "await-thenable")];
        apply(&mut diags);
        assert!(diags.is_empty(), "await-thenable in .spec.ts must be suppressed");
    }

    #[test]
    fn drops_await_thenable_in_tests_dir() {
        let mut diags = vec![diag("src/__tests__/helpers.ts", "await-thenable")];
        apply(&mut diags);
        assert!(diags.is_empty(), "await-thenable in __tests__ must be suppressed");
    }

    #[test]
    fn keeps_await_thenable_in_production_file() {
        let mut diags = vec![diag("src/features/product/product-row-actions.tsx", "await-thenable")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1, "await-thenable in production file must be kept");
    }

    #[test]
    fn does_not_touch_other_rules_in_test_files() {
        let mut diags = vec![
            diag("src/component.test.tsx", "await-thenable"),
            diag("src/component.test.tsx", "no-explicit-any"),
        ];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-explicit-any");
    }
}
