//! prefer-less-than Rust backend — flag `binary_expression` nodes using `>` or
//! `>=` whose left operand is strictly more constant-like than the right, and
//! suggest the equivalent `<` / `<=` form with operands swapped. Inversion only
//! improves readability for Yoda-style comparisons (`5 > x` → `x < 5`); when the
//! right operand is at least as constant-like (`MAX > 0`, `a > b`), the
//! comparison already reads as "subject before threshold" and swapping would
//! create the Yoda condition instead of removing it.

use super::Constness;
use crate::diagnostic::{Diagnostic, Severity};
use tree_sitter::Node;

/// Rank a tree-sitter operand node on the `Constness` scale.
fn constness(expr: Node, source: &[u8]) -> Constness {
    match expr.kind() {
        "integer_literal" | "float_literal" | "string_literal" | "raw_string_literal"
        | "boolean_literal" | "char_literal" => Constness::Literal,
        // Parentheses are transparent: `MAX > (0)` ranks like `MAX > 0`. A
        // comment can sit on either side here, so no index is extra-proof;
        // follow the repo's convention (`rust_helpers::operand_is_bool`).
        "parenthesized_expression" => expr
            .named_child(0)
            .map_or(Constness::Subject, |inner| constness(inner, source)),
        "unary_expression" => match expr.child(0).map(|op| op.kind()) {
            // `*LAZY_GLOBAL` reads through `Deref` at runtime — in practice a
            // lazily-initialised global — so it is the subject of the
            // comparison even when the name is SCREAMING_SNAKE_CASE.
            Some("*") => Constness::Subject,
            // `-1`, `!FLAG` — negating or inverting a value still reads as a
            // value, so the operand decides. Comments are named extras in this
            // grammar, so the operand is the *last* named child.
            _ => expr
                .named_child(expr.named_child_count().saturating_sub(1))
                .map_or(Constness::Subject, |operand| constness(operand, source)),
        },
        "identifier" => expr
            .utf8_text(source)
            .map_or(Constness::Subject, super::name_constness),
        // `module::CONST` — the final path segment determines constness.
        "scoped_identifier" => expr
            .child_by_field_name("name")
            .and_then(|name| name.utf8_text(source).ok())
            .map_or(Constness::Subject, super::name_constness),
        _ => Constness::Subject,
    }
}

crate::ast_check! { on ["binary_expression"] => |node, source, ctx, diagnostics|
    let Some(op_node) = node.child_by_field_name("operator") else { return };
    let op = op_node.utf8_text(source).unwrap_or("");

    let suggested = match op {
        ">" => "<",
        ">=" => "<=",
        _ => return,
    };

    let Some(lhs) = node.child_by_field_name("left") else { return };
    let Some(rhs) = node.child_by_field_name("right") else { return };
    if constness(lhs, source) <= constness(rhs, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-less-than".into(),
        message: format!(
            "Prefer `{suggested}` over `{op}` for readability \u{2014} swap operands and use `{suggested}`."
        ),
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
    fn flags_literal_left() {
        let d = run_on("fn f(x: i32) { if 5 > x { g(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<`"));
        assert_eq!(run_on("fn f(x: i32) -> bool { (5) > x }").len(), 1);
        assert_eq!(run_on("fn f(s: &str) -> bool { r\"z\" > s }").len(), 1);
    }

    #[test]
    fn flags_literal_left_greater_or_equal() {
        let d = run_on("fn f(x: i32) { if 5 >= x { g(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<=`"));
    }

    #[test]
    fn flags_constant_left() {
        let d = run_on("fn f(x: usize) { if MAX > x { g(); } }");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`<`"));
    }

    #[test]
    fn flags_scoped_constant_left() {
        let d = run_on("fn f(x: usize) { if limits::MAX_DIFF_LINES > x { g(); } }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_negative_literal_left() {
        let d = run_on("fn f(x: i32) { if -1 > x { g(); } }");
        assert_eq!(d.len(), 1);
    }

    // Issue #1456 regression: a variable/field/method/path subject on the left
    // already reads naturally; inverting would put the constant/computed value
    // first, which is less readable.
    #[test]
    fn allows_method_call_vs_constant() {
        assert!(run_on("fn f() -> bool { self.doc.len_lines() > MAX_DIFF_LINES }").is_empty());
    }

    #[test]
    fn allows_variable_vs_method_call() {
        assert!(
            run_on("fn f(anchor_col: usize, width: usize) { if anchor_col > self.max_diagnostic_start(width) { g(); } }")
                .is_empty()
        );
    }

    #[test]
    fn allows_field_vs_variable() {
        assert!(run_on("fn f(line: usize) { if hunk.end > line { g(); } }").is_empty());
    }

    #[test]
    fn allows_variable_vs_literal() {
        assert!(run_on("fn f(x: i32) { if x > 0 { g(); } }").is_empty());
        assert!(run_on("fn f(x: f64) { if x >= 1.0 { g(); } }").is_empty());
    }

    #[test]
    fn allows_non_constant_identifier_left() {
        assert!(run_on("fn f(a: i32, b: i32) -> bool { a > b }").is_empty());
        assert!(run_on("fn f(b: i32, a: i32) -> bool { b > a }").is_empty());
    }

    // Issue #6812 regression, from `jdx/mise`.
    #[test]
    fn allows_deref_constant_vs_literal() {
        assert!(run_on("fn f() -> bool { *env::TEST_TRANCHE_COUNT > 0 }").is_empty());
    }

    // Issue #6812, second snippet. A dereferenced global is read at runtime
    // through `Deref`, so it is the subject of the comparison however its
    // target is named; the paired non-deref case must still fire, or the `*`
    // arm has neutered the rule.
    #[test]
    fn deref_demotes_constant_to_subject() {
        assert!(run_on("fn f(n: usize) -> bool { *MAX > n }").is_empty());
        assert_eq!(run_on("fn f(n: usize) -> bool { MAX > n }").len(), 1);
        assert!(
            run_on("fn f(warn_version: Version) { if *crate::cli::version::V >= warn_version { g(); } }")
                .is_empty()
        );
    }

    // `MAX > 0` already reads "subject before threshold" — see `Constness`.
    #[test]
    fn allows_constant_vs_literal() {
        assert!(run_on("fn f() -> bool { MAX > 0 }").is_empty());
        assert!(run_on("fn f() -> bool { limits::MAX_DIFF_LINES >= 1 }").is_empty());
        assert!(run_on("fn f() -> bool { MAX > r\"z\" }").is_empty());
        assert!(run_on("fn f() -> bool { MAX > (0) }").is_empty());
    }

    // Neither side outranks the other, so no swap can improve it.
    #[test]
    fn allows_equal_rank_operands() {
        assert!(run_on("fn f() -> bool { 5 > 3 }").is_empty());
        assert!(run_on("fn f() -> bool { MAX > MIN }").is_empty());
    }

    #[test]
    fn flags_literal_vs_constant() {
        let d = run_on("fn f() -> bool { 5 > MAX }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_less_than() {
        assert!(run_on("fn f(a: i32, b: i32) -> bool { a < b }").is_empty());
    }

    #[test]
    fn allows_less_or_equal() {
        assert!(run_on("fn f(a: i32, b: i32) -> bool { a <= b }").is_empty());
    }

    #[test]
    fn allows_equality() {
        assert!(run_on("fn f(a: i32, b: i32) -> bool { a == b }").is_empty());
    }
}
