//! Post-filter dropping `typescript/no-unsafe-type-assertion` in test files.
//!
//! Test code legitimately casts minimal stubs to full library types
//! (`{} as AnyColumn`), mock return values (`vi.fn() as UseFormSetError<…>`),
//! and runtime values after an explicit guard
//! (`expect(x).toBeInstanceOf(Foo); (x as Foo).field`). These type-level
//! shortcuts are idiomatic in tests and have different requirements than
//! production code. The native assertion rules already skip `in_test_dir`;
//! this mirrors that for the delegated tsgolint rule. (Closes #573)

use crate::diagnostic::Diagnostic;

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.retain(|d| {
        if d.rule_id.as_ref() != "typescript/no-unsafe-type-assertion" {
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

    #[test]
    fn drops_in_test_file() {
        let mut diags = vec![diag(
            "src/api/from-better-auth.test.ts",
            "typescript/no-unsafe-type-assertion",
        )];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn drops_in_integration_test_file() {
        let mut diags = vec![diag(
            "src/db/authorization.integration.test.ts",
            "typescript/no-unsafe-type-assertion",
        )];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn keeps_in_production_file() {
        let mut diags = vec![diag(
            "src/api/from-better-auth.ts",
            "typescript/no-unsafe-type-assertion",
        )];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_touch_other_rules() {
        let mut diags = vec![diag("src/api/foo.test.ts", "no-explicit-any")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }
}
