//! node-no-process-env backend — flag `process.env` usage.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["member_expression"] prefilter = ["process"] => |node, source, ctx, diagnostics|
    let Some(obj) = node.child_by_field_name("object") else { return };
    let Some(prop) = node.child_by_field_name("property") else { return };

    if obj.kind() != "identifier" || obj.utf8_text(source).unwrap_or("") != "process" {
        return;
    }
    if prop.utf8_text(source).unwrap_or("") != "env" {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "node-no-process-env".into(),
        message: "Unexpected use of `process.env`. Centralize environment access in a config module.".into(),
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
    fn flags_process_env() {
        let d = run_on("const port = process.env.PORT;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("process.env"));
    }

    #[test]
    fn flags_process_env_bracket() {
        let d = run_on("const x = process.env['NODE_ENV'];");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_other_process_members() {
        assert!(run_on("process.exit(1);").is_empty());
    }
}
