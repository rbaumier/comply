//! elysia-deploy-no-graceful-shutdown backend — flag `.listen(` without graceful shutdown wiring.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] prefilter = [".listen"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if !ctx.source.contains(".listen(") {
        return;
    }
    // If the file already wires shutdown signals OR calls `.stop()`, accept it.
    if ctx.source.contains("SIGTERM") || ctx.source.contains("SIGINT") || ctx.source.contains(".stop()") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    let callee_text = callee.utf8_text(source).unwrap_or("");
    if !callee_text.ends_with(".listen") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-deploy-no-graceful-shutdown".into(),
        message: "Elysia `.listen()` without SIGTERM/SIGINT handler — in-flight requests will be dropped on shutdown.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_framework(source, &Check, "elysia")
    }

    #[test]
    fn flags_listen_without_shutdown() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().listen(3000);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_listen_with_sigterm_handler() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia().listen(3000);\nprocess.on('SIGTERM', () => app.stop());";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.listen(3000);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
