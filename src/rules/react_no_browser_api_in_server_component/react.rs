//! react-no-browser-api-in-server-component backend.
//!
//! Matches `member_expression` where the object is a known browser global.
//! Bare `typeof window` probes (common SSR guard) slip through because the
//! outer `unary_expression` isn't a member access.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::file_ctx::RscContext;

const BROWSER_GLOBALS: &[&str] = &[
    "window",
    "document",
    "localStorage",
    "sessionStorage",
    "navigator",
    "location",
];

fn is_browser_global(name: &str) -> bool {
    BROWSER_GLOBALS.contains(&name)
}

fn is_inside_typeof(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "unary_expression"
            && let Some(op) = parent.child(0)
            && op.utf8_text(source).ok() == Some("typeof")
        {
            return true;
        }
        current = parent.parent();
    }
    false
}

crate::ast_check! { on ["member_expression"] => |node, source, ctx, diagnostics|
    if ctx.file.rsc_context != RscContext::ServerComponent {
        return;
    }
    let Some(object) = node.child_by_field_name("object") else { return };
    if object.kind() != "identifier" {
        return;
    }
    let Ok(name) = object.utf8_text(source) else { return };
    if !is_browser_global(name) {
        return;
    }
    if is_inside_typeof(node, source) {
        return;
    }

    let pos = object.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-browser-api-in-server-component".into(),
        message: format!(
            "`{name}` is a browser global and doesn't exist on the server. \
             Gate this behind `\"use client\"` or a client-only boundary."
        ),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::FileCtx;

    fn server_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ServerComponent,
            ..Default::default()
        }
    }

    fn client_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ClientComponent,
            ..Default::default()
        }
    }

    fn run(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_file_ctx(source, &Check, file)
    }

    #[test]
    fn flags_window_access_in_server_component() {
        let src = r#"
export default function Page() {
    const w = window.innerWidth;
    return <div>{w}</div>;
}
"#;
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn flags_document_and_localstorage() {
        let src = r#"
export default function Page() {
    const el = document.getElementById("x");
    const v = localStorage.getItem("k");
    return <div />;
}
"#;
        assert_eq!(run(src, &server_ctx()).len(), 2);
    }

    #[test]
    fn allows_typeof_window_guard() {
        let src = r#"
export default function Page() {
    const isClient = typeof window !== "undefined";
    return <div>{String(isClient)}</div>;
}
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }

    #[test]
    fn allows_browser_globals_in_client_component() {
        let src = r#"
export default function Page() {
    const w = window.innerWidth;
    return <div>{w}</div>;
}
"#;
        assert!(run(src, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_non_browser_names() {
        let src = r#"
export default function Page({ window }) {
    return <div>{window.foo}</div>;
}
"#;
        // Parameter named `window` still matches — this rule doesn't do
        // scope analysis. The trade-off is acceptable: shadowing a browser
        // global with a param is a bad idea anyway.
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn ignores_unknown_rsc_context() {
        let src = r#"
export default function Page() {
    const w = window.innerWidth;
    return <div>{w}</div>;
}
"#;
        assert!(run(src, &FileCtx::default()).is_empty());
    }
}
