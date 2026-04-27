//! elysia-nodejs-adapter-required backend — flag `@elysiajs/node` import without `adapter:` configuration.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }
    if ctx.source.contains("adapter:") {
        return;
    }

    let text = node.utf8_text(source).unwrap_or("");
    if !text.contains("@elysiajs/node") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-nodejs-adapter-required".into(),
        message: "`@elysiajs/node` imported but no `adapter:` set on the Elysia constructor.".into(),
        severity: Severity::Error,
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
    fn flags_node_import_without_adapter() {
        let src = "import { node } from '@elysiajs/node';\nimport { Elysia } from 'elysia';\nnew Elysia().listen(3000);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_node_with_adapter() {
        let src = "import { node } from '@elysiajs/node';\nimport { Elysia } from 'elysia';\nnew Elysia({ adapter: node() }).listen(3000);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_node_files() {
        let src = "import { Elysia } from 'elysia';\nnew Elysia().listen(3000);";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
