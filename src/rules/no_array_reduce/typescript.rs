//! no-array-reduce backend — flag complex `.reduce()` / `.reduceRight()` calls.
//!
//! Simple arithmetic accumulations (sum, product, min, max) are allowed
//! because they're universally readable. Complex reduces that build
//! objects, filter, or nest logic should use a `for` loop instead.

use crate::diagnostic::{Diagnostic, Severity};

const METHODS: &[&str] = &["reduce", "reduceRight"];

/// Arithmetic operators that indicate a simple sum/product/min/max reduce.
const SIMPLE_OPS: &[&str] = &["+", "-", "*", "/", "%", "**"];

/// Check if the callback body is a simple arithmetic accumulation like
/// `(acc, x) => acc + x` or `(sum, n) => sum + n`. These are readable
/// enough to allow — the rule targets complex reduces that build objects
/// or nest conditionals.
fn is_simple_arithmetic(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = node.child_by_field_name("arguments") else {
        return false;
    };
    let callback = args
        .named_children(&mut args.walk())
        .find(|c| c.kind() == "arrow_function" || c.kind() == "function_expression");
    let Some(cb) = callback else { return false };

    // Get the callback body — for arrow functions it's either a direct
    // expression (concise body) or a statement_block.
    let Some(body) = cb.child_by_field_name("body") else {
        return false;
    };

    // Concise arrow: `(acc, x) => acc + x` — body is a binary_expression.
    if body.kind() == "binary_expression" {
        let op = body
            .child_by_field_name("operator")
            .map(|o| o.utf8_text(source).unwrap_or(""));
        return op.is_some_and(|o| SIMPLE_OPS.contains(&o));
    }

    // Block body with a single return: `(acc, x) => { return acc + x; }`
    if body.kind() == "statement_block" {
        let stmts: Vec<_> = body
            .named_children(&mut body.walk())
            .filter(|c| c.kind() != "comment")
            .collect();
        if stmts.len() == 1 && stmts[0].kind() == "return_statement" {
            let ret = stmts[0];
            let expr = ret.named_children(&mut ret.walk()).next();
            if let Some(e) = expr
                && e.kind() == "binary_expression"
            {
                let op = e
                    .child_by_field_name("operator")
                    .map(|o| o.utf8_text(source).unwrap_or(""));
                return op.is_some_and(|o| SIMPLE_OPS.contains(&o));
            }
        }
    }

    // Also allow Math.min/Math.max callbacks: `(a, b) => Math.min(a, b)`
    if body.kind() == "call_expression" {
        let callee_text = body
            .child_by_field_name("function")
            .map(|f| f.utf8_text(source).unwrap_or(""));
        if callee_text.is_some_and(|t| t == "Math.min" || t == "Math.max") {
            return true;
        }
    }

    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }

    let Some(prop) = callee.child_by_field_name("property") else { return };
    let method = prop.utf8_text(source).unwrap_or("");
    if !METHODS.contains(&method) {
        return;
    }

    // Allow simple arithmetic accumulations — they're readable as-is.
    if is_simple_arithmetic(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-array-reduce".into(),
        message: format!(
            "`Array#{}()` with complex logic is hard to read — use a `for...of` loop instead. \
             Simple arithmetic reduces like `(sum, n) => sum + n` are allowed.",
            method
        ),
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
    fn allows_simple_sum() {
        assert!(run_on("const sum = arr.reduce((acc, x) => acc + x, 0);").is_empty());
    }

    #[test]
    fn allows_simple_product() {
        assert!(run_on("const prod = arr.reduce((acc, x) => acc * x, 1);").is_empty());
    }

    #[test]
    fn allows_simple_sum_block_body() {
        assert!(run_on("const sum = arr.reduce((acc, x) => { return acc + x; }, 0);").is_empty());
    }

    #[test]
    fn allows_math_min() {
        assert!(run_on("const min = arr.reduce((a, b) => Math.min(a, b));").is_empty());
    }

    #[test]
    fn flags_complex_reduce() {
        assert_eq!(
            run_on("const obj = arr.reduce((acc, x) => ({ ...acc, [x.id]: x }), {});").len(),
            1
        );
    }

    #[test]
    fn flags_reduce_right_complex() {
        assert_eq!(
            run_on("const r = arr.reduceRight((acc, x) => acc.concat(x.items), []);").len(),
            1
        );
    }

    #[test]
    fn allows_non_reduce() {
        assert!(run_on("const x = arr.map(x => x * 2);").is_empty());
    }

    #[test]
    fn allows_direct_function_call() {
        assert!(run_on("reduce(acc, x);").is_empty());
    }
}
