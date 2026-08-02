//! non-existent-operator Rust backend.
//!
//! Detect the `=-` and `=!` typo operators. `x =- 1` parses as `x = -1`, an
//! assignment of a negated value, while the text reads as a single `-=` token.
//! Spacing decides — see [`super::reads_as_compact_assignment`].

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["assignment_expression"] => |node, source, ctx, diagnostics|
    let Some(rhs) = node.child_by_field_name("right") else { return };
    if rhs.kind() != "unary_expression" {
        return;
    }

    let Some(sign) = rhs.child(0) else { return };
    let sign_text = sign.utf8_text(source).unwrap_or("");
    let suggested = match sign_text {
        "-" => "-=",
        "!" => "!=",
        _ => return,
    };

    let mut cursor = node.walk();
    let Some(eq) = node.children(&mut cursor).find(|child| child.kind() == "=") else {
        return;
    };

    if eq.end_byte() != sign.start_byte() {
        return; // `x = -1` keeps them apart: a real unary sign.
    }

    // `x=-1` and `flag=!other` are compact assignments of a signed value.
    if super::reads_as_compact_assignment(ctx.source, eq.start_byte()) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "non-existent-operator".into(),
        message: format!("Typo: `={sign_text}` should be `{suggested}`."),
        severity: Severity::Error,
        span: None,
    });
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn allows_intentional_negative() {
        assert!(run_on("fn f() { let mut x = 0; x = -1; }").is_empty());
    }

    #[test]
    fn flags_typo_operators() {
        assert_eq!(run_on("fn f() { let mut x = 0; x =- 1; }").len(), 1);
        assert_eq!(run_on("fn f() { let mut x = 0; x=- 1; }").len(), 1);
        assert_eq!(
            run_on("fn f(o: bool) { let mut b = false; b =! o; }").len(),
            1
        );
    }

    /// A space on one side of the `=`/sign pair is enough to make their contact
    /// meaningful: no formatter writes an assignment that way.
    #[test]
    fn flags_half_spaced_typo_operators() {
        assert_eq!(run_on("fn f() { let mut x = 0; x =-1; }").len(), 1);
        assert_eq!(run_on("fn f(o: bool) { let mut b = false; b =!o; }").len(), 1);
    }

    /// Compact spacing glues every token to the next one, so the sign touching
    /// the `=` says nothing: it stays a unary sign on the value.
    #[test]
    fn allows_compact_unary_sign() {
        assert!(run_on("fn f() { let mut x = 0; x=-1; }").is_empty());
        assert!(run_on("fn f(o: bool) { let mut b = false; b=!o; }").is_empty());
        assert!(run_on("fn f() { let mut x = 0; x=-(1 + 2); }").is_empty());
    }
}
