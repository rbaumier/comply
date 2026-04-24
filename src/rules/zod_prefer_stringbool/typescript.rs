//! zod-prefer-stringbool backend — flag `z.coerce.boolean()` calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Ok(func_text) = func.utf8_text(source) else { return };
    if func_text != "z.coerce.boolean" { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: "`z.coerce.boolean()` treats every non-empty string as `true` — \
                  use `z.stringbool()` for HTML form inputs and query strings.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_coerce_boolean() {
        assert_eq!(run("const S = z.coerce.boolean();").len(), 1);
    }

    #[test]
    fn allows_stringbool() {
        assert!(run("const S = z.stringbool();").is_empty());
    }

    #[test]
    fn allows_coerce_number() {
        assert!(run("const S = z.coerce.number();").is_empty());
    }
}
