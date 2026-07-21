//! axum-jwt-missing-exp backend.
//!
//! Flags `<expr>.validate_exp = false` — turning off `jsonwebtoken`'s expiry
//! check on a `Validation` value, so tokens whose `exp` claim has already
//! passed are still accepted. `validate_exp` defaults to `true`
//! (`Validation::new(alg)` keeps it on); an explicit assignment of the literal
//! `false` is the only shape that disables expiry validation.
//!
//! Detection requires all of:
//!
//! 1. an `assignment_expression` whose left-hand side is a `field_expression`
//!    whose `field` is the identifier `validate_exp`, and
//! 2. a right-hand side that is the `boolean_literal` `false`.
//!
//! Reading `validate_exp`, comparing it, or assigning it any non-`false` value
//! (a variable, `true`, a config flag) is a different shape and is left alone —
//! the rule fires only on expiry validation being explicitly turned off.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["assignment_expression"] prefilter = ["validate_exp"] => |node, source, ctx, diagnostics|
    let Some(left) = node.child_by_field_name("left") else { return };
    if left.kind() != "field_expression" { return; }
    let assigns_validate_exp = left
        .child_by_field_name("field")
        .and_then(|f| f.utf8_text(source).ok())
        == Some("validate_exp");
    if !assigns_validate_exp { return; }

    let Some(right) = node.child_by_field_name("right") else { return };
    if right.kind() != "boolean_literal" || right.utf8_text(source).ok() != Some("false") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`validate_exp` is set to `false` — `jsonwebtoken` will accept expired tokens. \
         Leave it at its default (`true`) so expired tokens are rejected."
            .into(),
        Severity::Error,
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    // ── Positive: expiry validation explicitly disabled ─────────────────────

    #[test]
    fn flags_validate_exp_false() {
        // The exact "should flag" snippet from the issue body.
        let src = r#"fn f() { let mut v = Validation::new(Algorithm::HS256); v.validate_exp = false; }"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_validate_exp_false_on_field_receiver() {
        // A nested receiver (`cfg.validation`) still ends in the `validate_exp`
        // field being set to `false`.
        let src = r#"fn f(cfg: &mut Config) { cfg.validation.validate_exp = false; }"#;
        assert_eq!(run(src).len(), 1);
    }

    // ── Negative: expiry validation left on ─────────────────────────────────

    #[test]
    fn allows_default_validation() {
        // The exact "should not flag" snippet from the issue body.
        let src = r#"fn f() { let v = Validation::new(Algorithm::HS256); }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_validate_exp_true() {
        let src = r#"fn f() { let mut v = Validation::new(Algorithm::HS256); v.validate_exp = true; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_validate_exp_from_variable() {
        // Assigning a runtime flag is not an explicit `= false`.
        let src = r#"fn f(mut v: Validation, enabled: bool) { v.validate_exp = enabled; }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_reading_validate_exp() {
        // Reading the field is not an assignment.
        let src = r#"fn f(v: &Validation) -> bool { v.validate_exp }"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_unrelated_field_false() {
        // A different field assigned `false` is not the jsonwebtoken expiry flag.
        let src = r#"fn f(mut v: Validation) { v.validate_nbf = false; }"#;
        assert!(run(src).is_empty());
    }
}
