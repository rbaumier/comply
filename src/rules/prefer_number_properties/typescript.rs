//! prefer-number-properties backend — flag global `isNaN`, `parseInt`, etc.

use crate::diagnostic::{Diagnostic, Severity};

struct GlobalCheck {
    name: &'static str,
    is_call: bool,
    message: &'static str,
}

const CHECKS: &[GlobalCheck] = &[
    GlobalCheck {
        name: "isNaN",
        is_call: true,
        message: "Prefer `Number.isNaN()` over global `isNaN()`. `Number.isNaN()` does not coerce.",
    },
    GlobalCheck {
        name: "isFinite",
        is_call: true,
        message: "Prefer `Number.isFinite()` over global `isFinite()`. `Number.isFinite()` does not coerce.",
    },
    GlobalCheck {
        name: "parseInt",
        is_call: true,
        message: "Prefer `Number.parseInt()` over global `parseInt()`.",
    },
    GlobalCheck {
        name: "parseFloat",
        is_call: true,
        message: "Prefer `Number.parseFloat()` over global `parseFloat()`.",
    },
    GlobalCheck {
        name: "NaN",
        is_call: false,
        message: "Prefer `Number.NaN` over global `NaN`.",
    },
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "identifier" {
        return;
    }

    let name = node.utf8_text(source).unwrap_or("");

    let Some(chk) = CHECKS.iter().find(|c| c.name == name) else { return };

    // Must be a standalone identifier, not a member expression property.
    if let Some(parent) = node.parent()
        && parent.kind() == "member_expression"
            && let Some(prop) = parent.child_by_field_name("property")
                && prop.id() == node.id() {
                    // This identifier is the property of a member expression
                    // (e.g., `Number.isNaN`), not a standalone global.
                    return;
                }

    // For calls, verify the identifier is the function of a call_expression.
    if chk.is_call {
        let is_call = node.parent().is_some_and(|p| {
            p.kind() == "call_expression"
                && p.child_by_field_name("function")
                    .is_some_and(|f| f.id() == node.id())
        });
        if !is_call { return; }
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-number-properties".into(),
        message: chk.message.into(),
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
    fn flags_global_is_nan() {
        let d = run_on("if (isNaN(value)) {}");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-number-properties");
        assert!(d[0].message.contains("Number.isNaN"));
    }

    #[test]
    fn flags_global_parse_int() {
        let d = run_on("const n = parseInt('10', 10);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Number.parseInt"));
    }

    #[test]
    fn flags_global_parse_float() {
        let d = run_on("const n = parseFloat('3.14');");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_global_is_finite() {
        let d = run_on("if (isFinite(x)) {}");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_global_nan() {
        let d = run_on("const x = NaN;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Number.NaN"));
    }

    #[test]
    fn allows_number_is_nan() {
        assert!(run_on("if (Number.isNaN(value)) {}").is_empty());
    }

    #[test]
    fn allows_number_parse_int() {
        assert!(run_on("const n = Number.parseInt('10', 10);").is_empty());
    }

    #[test]
    fn ignores_member_access() {
        assert!(run_on("foo.isNaN(value);").is_empty());
    }
}
