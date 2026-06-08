//! prefer-math-min-max — flag comparison ternaries replaceable by Math.min/max.
//!
//! Patterns detected (where `>` can be `>=`, `<` can be `<=`):
//!
//! Math.min:
//! - `height > 50 ? 50 : height`  (greater, left=alt, right=cons)
//! - `height < 50 ? height : 50`  (less, left=cons, right=alt)
//!
//! Math.max:
//! - `height > 50 ? height : 50`  (greater, left=cons, right=alt)
//! - `height < 50 ? 50 : height`  (less, left=alt, right=cons)

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["ternary_expression"] => |node, source, ctx, diagnostics|
    let test = match node.child_by_field_name("condition") {
        Some(c) => c,
        None => return,
    };

    // The condition must be a binary comparison.
    if test.kind() != "binary_expression" {
        return;
    }

    let op_node = match test.child_by_field_name("operator") {
        Some(o) => o,
        None => return,
    };
    let op = op_node.utf8_text(source).unwrap_or("");

    let is_gt = op == ">" || op == ">=";
    let is_lt = op == "<" || op == "<=";
    if !is_gt && !is_lt {
        return;
    }

    let left = match test.child_by_field_name("left") {
        Some(l) => l,
        None => return,
    };
    let right = match test.child_by_field_name("right") {
        Some(r) => r,
        None => return,
    };
    let consequent = match node.child_by_field_name("consequence") {
        Some(c) => c,
        None => return,
    };
    let alternate = match node.child_by_field_name("alternative") {
        Some(a) => a,
        None => return,
    };

    let left_text = left.utf8_text(source).unwrap_or("").trim();
    let right_text = right.utf8_text(source).unwrap_or("").trim();
    let cons_text = consequent.utf8_text(source).unwrap_or("").trim();
    let alt_text = alternate.utf8_text(source).unwrap_or("").trim();

    if left_text.is_empty() || right_text.is_empty() {
        return;
    }

    let method: Option<&str> = if
        // Math.min: `a > b ? b : a` or `a < b ? a : b`
        (is_gt && left_text == alt_text && right_text == cons_text)
        || (is_lt && left_text == cons_text && right_text == alt_text)
    {
        Some("min")
    } else if
        // Math.max: `a > b ? a : b` or `a < b ? b : a`
        (is_gt && left_text == cons_text && right_text == alt_text)
        || (is_lt && left_text == alt_text && right_text == cons_text)
    {
        Some("max")
    } else {
        None
    };

    if let Some(method) = method {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "prefer-math-min-max".into(),
            message: format!(
                "Prefer `Math.{method}({left_text}, {right_text})` over this ternary."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
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

    // --- Math.min ---

    #[test]
    fn flags_gt_min_pattern() {
        // height > 50 ? 50 : height -> Math.min(height, 50)
        let d = run_on("const x = height > 50 ? 50 : height;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.min"));
    }

    #[test]
    fn flags_lt_min_pattern() {
        // height < 50 ? height : 50 -> Math.min(height, 50)
        let d = run_on("const x = height < 50 ? height : 50;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.min"));
    }

    #[test]
    fn flags_gte_min_pattern() {
        let d = run_on("const x = height >= 50 ? 50 : height;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.min"));
    }

    // --- Math.max ---

    #[test]
    fn flags_gt_max_pattern() {
        // height > 50 ? height : 50 -> Math.max(height, 50)
        let d = run_on("const x = height > 50 ? height : 50;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.max"));
    }

    #[test]
    fn flags_lt_max_pattern() {
        // height < 50 ? 50 : height -> Math.max(height, 50)
        let d = run_on("const x = height < 50 ? 50 : height;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Math.max"));
    }

    // --- No match ---

    #[test]
    fn allows_unrelated_ternary() {
        assert!(run_on("const x = a > b ? c : d;").is_empty());
    }

    #[test]
    fn allows_equality_ternary() {
        assert!(run_on("const x = a === b ? a : b;").is_empty());
    }

    #[test]
    fn allows_already_using_math_min() {
        assert!(run_on("const x = Math.min(a, b);").is_empty());
    }
}
