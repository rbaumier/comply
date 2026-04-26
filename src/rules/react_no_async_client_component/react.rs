//! react-no-async-client-component backend.
//!
//! Flags exported React components declared `async` inside files classified
//! as `RscContext::ClientComponent`. A component is recognised by an
//! uppercase-leading name, matching React's naming convention.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::file_ctx::RscContext;

fn first_token_is_async(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Ok(text) = node.utf8_text(source) else { return false };
    text.trim_start().starts_with("async ")
}

fn starts_with_uppercase(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

fn is_inside_export(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "export_statement" {
            return true;
        }
        // Stop walking once we exit the top-level program.
        if parent.kind() == "program" {
            return false;
        }
        current = parent.parent();
    }
    false
}

crate::ast_check! { on ["function_declaration"] => |node, source, ctx, diagnostics|
    if ctx.file.rsc_context != RscContext::ClientComponent {
        return;
    }
    if !is_inside_export(node) {
        return;
    }
    if !first_token_is_async(node, source) {
        return;
    }
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let Ok(name) = name_node.utf8_text(source) else { return };
    if !starts_with_uppercase(name) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-async-client-component".into(),
        message: format!(
            "`{name}` is an async client component. React client components \
             must be synchronous — remove `async`, or drop `\"use client\"` \
             to make this a server component."
        ),
        severity: Severity::Error,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::FileCtx;

    fn client_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ClientComponent,
            ..Default::default()
        }
    }

    fn server_ctx() -> FileCtx {
        FileCtx {
            rsc_context: RscContext::ServerComponent,
            ..Default::default()
        }
    }

    fn run(source: &str, file: &FileCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx_with_file_ctx(source, &Check, file)
    }

    #[test]
    fn flags_default_async_component() {
        let src = r#"
"use client";

export default async function Page() {
    return <div />;
}
"#;
        assert_eq!(run(src, &client_ctx()).len(), 1);
    }

    #[test]
    fn flags_named_async_component() {
        let src = r#"
"use client";

export async function UserCard() {
    return <div />;
}
"#;
        assert_eq!(run(src, &client_ctx()).len(), 1);
    }

    #[test]
    fn allows_sync_component_in_client() {
        let src = r#"
"use client";

export default function Page() {
    return <div />;
}
"#;
        assert!(run(src, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_async_component_in_server() {
        let src = r#"
export default async function Page() {
    const data = await fetchData();
    return <div>{data}</div>;
}
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }

    #[test]
    fn allows_async_utility_exported_from_client() {
        // `loadData` is lowercase → not a component.
        let src = r#"
"use client";

export async function loadData() {
    return 42;
}
"#;
        assert!(run(src, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_async_not_exported() {
        let src = r#"
"use client";

async function Helper() {
    return null;
}

export default function Page() { return <div />; }
"#;
        assert!(run(src, &client_ctx()).is_empty());
    }
}
