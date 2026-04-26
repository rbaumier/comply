use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["as_expression"] => |node, source, ctx, diagnostics|
    // Allow `as const` — it's a type refinement, not a cast
    let node_text = node.utf8_text(source).unwrap_or("");
    if node_text.trim_end().ends_with("as const") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-type-assertion".into(),
        message: "Type assertion `as T` bypasses the type checker — use `satisfies`, type guards, or generics.".into(),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(code, &Check)
    }

    #[test]
    fn flags_as_expression() {
        let diags = run_on("const x = foo as string;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("as T"));
    }

    #[test]
    fn flags_as_any() {
        let diags = run_on("const x = foo as any;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_as_unknown() {
        let diags = run_on("const x = foo as unknown;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_double_assertion() {
        let diags = run_on("const x = foo as unknown as Bar;");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn allows_as_const() {
        let diags = run_on("const x = { a: 1 } as const;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_as_const_multiline() {
        let code = "const x = { a: 1 } as const;\nconst y = 2;";
        let diags = run_on(code);
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_as_const_array() {
        let diags = run_on("const arr = [1, 2, 3] as const;");
        assert!(diags.is_empty(), "unexpected diags: {:?}", diags);
    }

    #[test]
    fn allows_satisfies() {
        let diags = run_on("const x = { a: 1 } satisfies Config;");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_generic_call() {
        let diags = run_on("const x = getValue<string>();");
        assert!(diags.is_empty());
    }
}
