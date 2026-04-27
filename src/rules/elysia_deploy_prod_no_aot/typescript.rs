//! elysia-deploy-prod-no-aot backend — flag Elysia instances missing `aot:true`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["new_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(constructor) = node.child_by_field_name("constructor") else { return };
    if constructor.utf8_text(source).unwrap_or("") != "Elysia" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    // Only flag when the constructor receives a config object — bare `new Elysia()` is fine.
    if !norm.contains('{') {
        return;
    }
    if norm.contains("aot:true") || norm.contains("aot:false") {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-deploy-prod-no-aot".into(),
        message: "`new Elysia({ ... })` does not set `aot` — for production deployments, set `aot: true` to enable ahead-of-time compilation.".into(),
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
    fn flags_config_without_aot() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia({ prefix: '/v1' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_named_config_without_aot() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia({ name: 'auth' });";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_aot_true() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia({ aot: true });";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_constructor() {
        let src = "import { Elysia } from 'elysia';\nconst app = new Elysia();";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_elysia_files() {
        let src = "const app = new Elysia({ prefix: '/v1' });";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
