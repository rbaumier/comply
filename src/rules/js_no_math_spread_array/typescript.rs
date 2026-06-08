//! Flags `Math.min(...arr)` and `Math.max(...arr)` — JS engines cap the
//! number of arguments a call can receive (typically ~65k–100k); spreading
//! a large array overflows the call stack.

use crate::diagnostic::{Diagnostic, Severity};

fn callee_is_math_min_or_max<'a>(
    callee: tree_sitter::Node<'a>,
    source: &'a [u8],
) -> Option<&'a str> {
    if callee.kind() != "member_expression" {
        return None;
    }
    let object = callee.child_by_field_name("object")?;
    if object.kind() != "identifier" {
        return None;
    }
    if object.utf8_text(source).ok() != Some("Math") {
        return None;
    }
    let prop = callee.child_by_field_name("property")?;
    let name = prop.utf8_text(source).ok()?;
    if name == "min" || name == "max" {
        Some(name)
    } else {
        None
    }
}

fn arguments_contain_spread(arguments: tree_sitter::Node) -> bool {
    let mut cursor = arguments.walk();
    arguments
        .children(&mut cursor)
        .any(|c| c.kind() == "spread_element")
}

crate::ast_check! { on ["call_expression"] prefilter = ["Math"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    let Some(method) = callee_is_math_min_or_max(callee, source) else { return };

    let Some(args) = node.child_by_field_name("arguments") else { return };
    if !arguments_contain_spread(args) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`Math.{method}(...array)` overflows the stack on large arrays — \
             use `reduce` or a for-loop instead."
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
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_math_min_spread() {
        assert_eq!(run(r#"const m = Math.min(...values);"#).len(), 1);
    }

    #[test]
    fn flags_math_max_spread() {
        assert_eq!(run(r#"const m = Math.max(...values);"#).len(), 1);
    }

    #[test]
    fn flags_math_max_spread_with_other_args() {
        assert_eq!(run(r#"const m = Math.max(0, ...values);"#).len(), 1);
    }

    #[test]
    fn allows_math_min_literal_args() {
        assert!(run(r#"const m = Math.min(1, 2, 3);"#).is_empty());
    }

    #[test]
    fn allows_math_max_literal_args() {
        assert!(run(r#"const m = Math.max(a, b);"#).is_empty());
    }

    #[test]
    fn allows_other_math_with_spread() {
        assert!(run(r#"const m = Math.hypot(...values);"#).is_empty());
    }
}
