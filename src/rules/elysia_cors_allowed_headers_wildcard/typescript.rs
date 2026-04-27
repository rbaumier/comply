//! elysia-cors-allowed-headers-wildcard backend — flag wildcard allowedHeaders with credentials.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("elysia") {
        return;
    }

    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.utf8_text(source).unwrap_or("") != "cors" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let args_text = args.utf8_text(source).unwrap_or("");
    let norm: String = args_text.chars().filter(|c| !c.is_whitespace()).collect();
    if !norm.contains("credentials:true") {
        return;
    }

    let wildcard = norm.contains("allowedHeaders:'*'") || norm.contains("allowedHeaders:\"*\"");
    let omitted = !norm.contains("allowedHeaders:");
    if !wildcard && !omitted {
        return;
    }

    let pos = node.start_position();
    let msg = if wildcard {
        "`cors({ credentials: true, allowedHeaders: '*' })` is rejected by browsers — list explicit headers."
    } else {
        "`cors({ credentials: true })` without `allowedHeaders` falls back to the wildcard, which browsers reject — list explicit headers."
    };
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "elysia-cors-allowed-headers-wildcard".into(),
        message: msg.into(),
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
    fn flags_wildcard_with_credentials() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ credentials: true, allowedHeaders: '*' }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_omitted_with_credentials() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ credentials: true, origin: 'https://x.example' }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_explicit_headers() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ credentials: true, allowedHeaders: ['content-type', 'authorization'] }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_credentials_false() {
        let src = "import { cors } from '@elysiajs/cors';\napp.use(cors({ credentials: false }));";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_cors_files() {
        let src = "app.use(cors({ credentials: true }));";
        assert!(crate::rules::test_helpers::run_ts(src, &Check).is_empty());
    }
}
