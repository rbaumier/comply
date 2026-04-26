//! zod-no-empty-custom-schema backend — flag `z.custom()` with no arguments.
//!
//! `z.custom<T>()` without a validator function performs no runtime check —
//! it asserts the TypeScript type without verifying the value. Pass a
//! validator function (e.g. `z.custom<T>((v) => typeof v === "string")`)
//! so the schema actually validates at runtime.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if name != "z.custom" && name != "zod.custom" {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    if args.named_child_count() != 0 {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-no-empty-custom-schema".into(),
        message: "`z.custom()` without a validator function performs no runtime check — provide a validator function to z.custom().".into(),
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
    fn flags_empty_z_custom() {
        assert_eq!(run_on("const s = z.custom();").len(), 1);
    }

    #[test]
    fn flags_empty_z_custom_with_type_arg() {
        assert_eq!(run_on("const s = z.custom<string>();").len(), 1);
    }

    #[test]
    fn flags_empty_zod_custom() {
        assert_eq!(run_on("const s = zod.custom();").len(), 1);
    }

    #[test]
    fn allows_z_custom_with_validator() {
        assert!(
            run_on("const s = z.custom<string>((v) => typeof v === 'string');").is_empty()
        );
    }

    #[test]
    fn allows_unrelated_calls() {
        assert!(run_on("const s = z.string();").is_empty());
        assert!(run_on("const s = custom();").is_empty());
    }
}
