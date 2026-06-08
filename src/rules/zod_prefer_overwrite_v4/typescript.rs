//! zod-prefer-overwrite-v4 backend — flag `.transform(fn)` calls where `fn` is a
//! single-parameter callback whose body returns a value trivially of the same
//! shape as the parameter (e.g. `s => s.trim()`, `(n) => Math.round(n)`).
//!
//! We use text heuristics on the arrow's source rather than walking its AST:
//! the same-shape check already reduces to string-shape anyway (e.g. body
//! starts with `<param>.`), so an extra layer of AST traversal only adds
//! brittleness around the `parameters` field, which varies in tree-sitter
//! grammar versions for single-identifier arrow params.

use crate::diagnostic::{Diagnostic, Severity};

/// Extract the parameter name from an arrow function source like
/// `s => s.trim()` or `(s) => s.trim()`. Returns `None` if the arrow has
/// zero or multiple parameters (we only target the 1-arg case).
fn extract_single_param(arrow_src: &str) -> Option<&str> {
    // Find the `=>`.
    let arrow_idx = arrow_src.find("=>")?;
    let head = arrow_src[..arrow_idx].trim();
    // Strip a leading `async` keyword.
    let head = head.strip_prefix("async").map(str::trim).unwrap_or(head);
    if let Some(rest) = head.strip_prefix('(') {
        let inner = rest.strip_suffix(')')?.trim();
        // No commas → single parameter.
        if inner.is_empty() || inner.contains(',') {
            return None;
        }
        // Drop any type annotation: `x: T` → `x`.
        let name = inner.split(':').next()?.trim();
        if !is_ident(name) {
            return None;
        }
        Some(name)
    } else {
        if is_ident(head) { Some(head) } else { None }
    }
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// Return true when `expr` is a shape-preserving expression for `param`.
fn is_same_shape_expr(expr_text: &str, param: &str) -> bool {
    let t = expr_text.trim().trim_end_matches(';');
    // Bare identifier identity (`s => s`).
    if t == param {
        return true;
    }
    // `param.xxx(...)` — method call on the parameter.
    if let Some(rest) = t.strip_prefix(param)
        && rest.starts_with('.')
    {
        return true;
    }
    // `Math.round(param)` / `Math.floor(param)` / `Math.ceil(param)` / ...
    for fun in [
        "Math.round",
        "Math.floor",
        "Math.ceil",
        "Math.abs",
        "Math.trunc",
    ] {
        if t.starts_with(fun) && t.contains(param) {
            return true;
        }
    }
    // Arithmetic on the parameter: `param + N`, `param - N`, `param * N`, `param / N`.
    // Conservative: the operand on the other side must be a numeric literal so we
    // know the result stays the same primitive type as `param`.
    if let Some(rest) = t.strip_prefix(param) {
        let rest = rest.trim_start();
        for op in ['+', '-', '*', '/'] {
            if let Some(rhs) = rest.strip_prefix(op) {
                let rhs = rhs.trim();
                if !rhs.is_empty() && rhs.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    return true;
                }
            }
        }
        // Nullish coalescing: `param ?? defaultValue`.
        if rest.trim_start().starts_with("??") {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Some(prop) = func.child_by_field_name("property") else { return };
    if prop.utf8_text(source).map(|t| t != "transform").unwrap_or(true) { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut ac = args.walk();
    let arrow = args.named_children(&mut ac).find(|c| c.kind() == "arrow_function");
    let Some(arrow) = arrow else { return };

    let Ok(arrow_src) = arrow.utf8_text(source) else { return };
    let Some(param) = extract_single_param(arrow_src) else { return };
    let param = param.to_string();

    let Some(body) = arrow.child_by_field_name("body") else { return };
    let Ok(body_text) = body.utf8_text(source) else { return };

    let same_shape = if body.kind() == "statement_block" {
        // Look for a single `return <expr>;` inside the block.
        let mut c = body.walk();
        let mut returns: Vec<String> = Vec::new();
        for child in body.named_children(&mut c) {
            if child.kind() == "return_statement"
                && let Some(e) = child.named_child(0)
                    && let Ok(t) = e.utf8_text(source) { returns.push(t.to_string()); }
        }
        returns.len() == 1 && is_same_shape_expr(&returns[0], &param)
    } else {
        is_same_shape_expr(body_text, &param)
    };

    if !same_shape { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`.transform()` returns the same-shape value as its input — \
                  use `.overwrite()` (Zod v4) to keep the input type intact.".into(),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_trim_transform() {
        assert_eq!(
            run("const S = z.string().transform(s => s.trim());").len(),
            1
        );
    }

    #[test]
    fn flags_math_round_transform() {
        assert_eq!(
            run("const S = z.number().transform(n => Math.round(n));").len(),
            1
        );
    }

    #[test]
    fn allows_overwrite() {
        assert!(run("const S = z.string().overwrite(s => s.trim());").is_empty());
    }

    #[test]
    fn ignores_shape_changing_transform() {
        // `.length` changes type from string to number — not same shape.
        assert!(run("const S = z.string().transform(s => ({ len: s.length }));").is_empty());
    }

    #[test]
    fn flags_identity_transform() {
        assert_eq!(run("const S = z.string().transform(s => s);").len(), 1);
    }

    #[test]
    fn flags_to_lower_case_transform() {
        assert_eq!(
            run("const S = z.string().transform(s => s.toLowerCase());").len(),
            1
        );
    }

    #[test]
    fn flags_arithmetic_transform() {
        assert_eq!(run("const S = z.number().transform(n => n + 1);").len(), 1);
    }

    #[test]
    fn flags_nullish_coalesce_transform() {
        assert_eq!(
            run("const S = z.string().transform(s => s ?? '');").len(),
            1
        );
    }
}
