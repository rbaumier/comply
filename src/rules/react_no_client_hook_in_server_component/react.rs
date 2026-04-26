//! react-no-client-hook-in-server-component backend.
//!
//! Flags `useFoo(...)` style call sites inside files classified as
//! `RscContext::ServerComponent`. React's `use()` primitive (lowercase, no
//! capital follow-up) is server-safe and slips through the `use[A-Z]` gate
//! intentionally.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::file_ctx::RscContext;

fn is_hook_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next() == Some('u')
        && chars.next() == Some('s')
        && chars.next() == Some('e')
        && chars.next().is_some_and(|c| c.is_ascii_uppercase())
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if ctx.file.rsc_context != RscContext::ServerComponent {
        return;
    }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "identifier" {
        return;
    }
    let Ok(name) = func.utf8_text(source) else { return };
    if !is_hook_name(name) {
        return;
    }

    let pos = func.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-client-hook-in-server-component".into(),
        message: format!(
            "`{name}()` is a React hook and can't run in a server component. \
             Mark the file with `\"use client\"` or extract this into a \
             client component."
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
    fn flags_use_state_in_server_component() {
        let src = r#"
export default function Page() {
    const [count, setCount] = useState(0);
    return <div>{count}</div>;
}
"#;
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn flags_use_router_in_server_component() {
        let src = r#"
export default function Page() {
    const router = useRouter();
    return <div />;
}
"#;
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn allows_use_state_in_client_component() {
        let src = r#"
export default function Page() {
    const [count, setCount] = useState(0);
    return <div>{count}</div>;
}
"#;
        assert!(run(src, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_plain_use_primitive_in_server_component() {
        // React 19 `use()` is server-safe — lowercase letter after "use"
        // means it's not a hook.
        let src = r#"
export default function Page() {
    const data = use(fetchData());
    return <div>{data}</div>;
}
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }

    #[test]
    fn allows_non_hook_named_function_in_server_component() {
        // `user` and `useless` don't match the `use[A-Z]` pattern.
        let src = r#"
export default function Page() {
    const u = user();
    const v = useless();
    return <div />;
}
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }

    #[test]
    fn ignores_unknown_rsc_context() {
        let src = r#"
export default function Page() {
    const [count, setCount] = useState(0);
    return <div>{count}</div>;
}
"#;
        assert!(run(src, &FileCtx::default()).is_empty());
    }
}
