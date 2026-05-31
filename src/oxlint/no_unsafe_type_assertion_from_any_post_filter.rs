//! Post-filter for `typescript/no-unsafe-type-assertion` false positives when
//! asserting *from* `any` to a concrete type.
//!
//! Libraries expose intentionally-open extensibility points typed `any` (e.g.
//! Base UI's `useToastManager<Data extends object = any>()` makes `toast.data`
//! `any`). Asserting that `any` to a concrete shape is the canonical typed-
//! access pattern — `any` has already opted out of type safety, so the cast
//! *adds* type information rather than removing it. tsgolint flags every
//! assertion from `any` regardless.
//!
//! tsgolint emits two distinct messages for this rule:
//!   - "Unsafe assertion from <type> detected: …"  (the source type)
//!   - "Unsafe assertion to <type> detected: …"    (the asserted type)
//! Drop only the `from any` variant; the `to <type>` variant (asserting *to*
//! an unsafe type — the genuinely dangerous direction) still fires. (Closes #572)

use crate::diagnostic::Diagnostic;

pub fn apply(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.retain(|d| {
        !(d.rule_id.as_ref() == "typescript/no-unsafe-type-assertion"
            && is_assertion_from_any(&d.message))
    });
}

/// True for tsgolint's "Unsafe assertion from any detected: …" message.
fn is_assertion_from_any(message: &str) -> bool {
    let Some(rest) = message.strip_prefix("Unsafe assertion from ") else {
        return false;
    };
    let Some(end) = rest.find(" detected") else {
        return false;
    };
    rest[..end].trim().trim_matches('`').trim() == "any"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Severity;
    use std::borrow::Cow;
    use std::path::Path;

    fn diag(rule: &'static str, message: &str) -> Diagnostic {
        Diagnostic {
            path: std::sync::Arc::from(Path::new("/tmp/x.ts")),
            line: 1,
            column: 1,
            rule_id: Cow::Borrowed(rule),
            message: message.to_string(),
            severity: Severity::Error,
            span: None,
        }
    }

    #[test]
    fn drops_assertion_from_any() {
        let mut diags = vec![diag(
            "typescript/no-unsafe-type-assertion",
            "Unsafe assertion from any detected: consider using type guards or a safer assertion.",
        )];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn drops_assertion_from_backticked_any() {
        let mut diags = vec![diag(
            "typescript/no-unsafe-type-assertion",
            "Unsafe assertion from `any` detected: consider using type guards or a safer assertion.",
        )];
        apply(&mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn keeps_assertion_to_unsafe_type() {
        let mut diags = vec![diag(
            "typescript/no-unsafe-type-assertion",
            "Unsafe assertion to Foo detected: consider using a more specific type to ensure safety.",
        )];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn keeps_assertion_from_non_any_source() {
        let mut diags = vec![diag(
            "typescript/no-unsafe-type-assertion",
            "Unsafe assertion from string detected: consider using type guards or a safer assertion.",
        )];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_touch_other_rules() {
        let mut diags = vec![diag("no-explicit-any", "Unsafe assertion from any detected: x")];
        apply(&mut diags);
        assert_eq!(diags.len(), 1);
    }
}
