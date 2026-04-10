//! zod-no-any backend — flag `z.any()`.
//!
//! Why: `z.any()` accepts anything — it's a type escape hatch that
//! disables validation entirely. Use `z.unknown()` instead: the runtime
//! result is the same, but the TypeScript type is `unknown`, forcing
//! downstream code to narrow before using the value.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some(name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if name != "z.any" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "zod-no-any".into(),
        message: "`z.any()` disables validation — use `z.unknown()` so the \
                  TypeScript type forces downstream code to narrow before \
                  using the value."
            .into(),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_z_any() {
        assert_eq!(run_on("const s = z.any();").len(), 1);
    }

    #[test]
    fn allows_z_unknown() {
        assert!(run_on("const s = z.unknown();").is_empty());
    }
}
