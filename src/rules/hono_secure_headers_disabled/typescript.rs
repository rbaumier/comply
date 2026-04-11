//! hono-secure-headers-disabled backend — flag explicitly disabled security headers.

use crate::diagnostic::{Diagnostic, Severity};

const SECURITY_HEADERS: &[&str] = &[
    "strictTransportSecurity",
    "xFrameOptions",
    "xContentTypeOptions",
    "removePoweredBy",
    "referrerPolicy",
];

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "pair" {
        return;
    }

    // Only check files that import from 'hono/secure-headers'.
    if !ctx.source.contains("hono/secure-headers") {
        return;
    }

    let Some(key) = node.child_by_field_name("key") else { return };
    let key_text = key.utf8_text(source).unwrap_or("");

    if !SECURITY_HEADERS.contains(&key_text) {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };
    let value_text = value.utf8_text(source).unwrap_or("");

    if value_text.trim() == "false" {
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "hono-secure-headers-disabled".into(),
            message: format!("`{}` is explicitly disabled — this removes a security protection.", key_text),
            severity: Severity::Error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_disabled_hsts() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\napp.use(secureHeaders({\n  strictTransportSecurity: false\n}));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_disabled_x_frame_options() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\napp.use(secureHeaders({ xFrameOptions: false }));";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_multiple_disabled() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\nsecureHeaders({\n  xFrameOptions: false,\n  removePoweredBy: false\n});";
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_default_secure_headers() {
        let src = "import { secureHeaders } from 'hono/secure-headers';\napp.use(secureHeaders());";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "secureHeaders({ xFrameOptions: false });";
        assert!(run_on(src).is_empty());
    }
}
