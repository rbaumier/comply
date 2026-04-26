//! zod-no-number-schema-with-int backend — flag `z.number().int()`.
//!
//! Zod v4 exposes `z.int()` as a dedicated integer schema. The legacy
//! `z.number().int()` chain creates a number schema and then refines it,
//! which is slower and more verbose than the direct `z.int()` schema.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // The outer call must be `<something>.int()`.
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" { return; }
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    let Ok("int") = prop.utf8_text(source) else { return; };

    // The object being `.int()`-ed must itself be `z.number()`.
    let Some(object) = callee.child_by_field_name("object") else { return; };
    if object.kind() != "call_expression" { return; }
    let Some(inner_fn) = object.child_by_field_name("function") else { return; };
    let Ok(inner_text) = inner_fn.utf8_text(source) else { return; };
    if inner_text != "z.number" { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-no-number-schema-with-int".into(),
        message: "`z.number().int()` can be replaced by `z.int()` in Zod v4+."
            .into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_z_number_int() {
        assert_eq!(run_on("const s = z.number().int();").len(), 1);
    }

    #[test]
    fn allows_z_int() {
        assert!(run_on("const s = z.int();").is_empty());
    }

    #[test]
    fn allows_z_number_positive() {
        assert!(run_on("const s = z.number().positive();").is_empty());
    }
}
