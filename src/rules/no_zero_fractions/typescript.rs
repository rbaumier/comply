//! no-zero-fractions backend — flag `1.0`, `2.00`, `3.` number literals.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["number"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");

    // Must contain a dot to be a decimal literal.
    let Some(dot_pos) = text.find('.') else { return };

    // Skip range operator `..` (shouldn't appear in a number node, but guard).
    if text.get(dot_pos + 1..dot_pos + 2) == Some(".") {
        return;
    }

    let fraction = &text[dot_pos + 1..];

    // Dangling dot: `1.` — fraction is empty.
    let is_dangling = fraction.is_empty();

    // Zero fraction: `1.0`, `1.00`, `1.0_0` — fraction is all zeros/underscores.
    let is_zero_fraction = !is_dangling
        && fraction.chars().all(|c| c == '0' || c == '_');

    if !is_dangling && !is_zero_fraction {
        return;
    }

    let msg = if is_dangling {
        "Don't use a dangling dot in the number."
    } else {
        "Don't use a zero fraction in the number."
    };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-zero-fractions".into(),
        message: msg.into(),
        severity: Severity::Warning,
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_zero_fraction() {
        let d = run_on("const x = 1.0;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("zero fraction"));
    }

    #[test]
    fn flags_multiple_trailing_zeros() {
        assert_eq!(run_on("const x = 1.00;").len(), 1);
    }

    #[test]
    fn allows_real_fraction() {
        assert!(run_on("const x = 1.5;").is_empty());
    }

    #[test]
    fn allows_integer() {
        assert!(run_on("const x = 1;").is_empty());
    }

    #[test]
    fn allows_non_zero_fraction() {
        assert!(run_on("const x = 3.14;").is_empty());
    }
}
