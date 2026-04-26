//! react-no-event-handler-in-server-component backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::file_ctx::RscContext;
use crate::rules::jsx::jsx_attribute_name;

fn is_event_handler(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next() == Some('o')
        && chars.next() == Some('n')
        && chars.next().is_some_and(|c| c.is_ascii_uppercase())
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if ctx.file.rsc_context != RscContext::ServerComponent {
        return;
    }
    let Some(attr_name) = jsx_attribute_name(node, source) else { return };
    if !is_event_handler(attr_name) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-event-handler-in-server-component".into(),
        message: format!(
            "`{attr_name}` is a client-side event handler. Server components \
             can't attach them — move this JSX into a `\"use client\"` \
             component or use `<form action={{...}}>` for submits."
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
    fn flags_on_click_in_server_component() {
        let src = r#"
export default function Page() {
    return <button onClick={() => {}}>X</button>;
}
"#;
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn flags_on_change_on_submit() {
        let src = r#"
export default function Page() {
    return (
        <form onSubmit={handle}>
            <input onChange={noop} />
        </form>
    );
}
"#;
        assert_eq!(run(src, &server_ctx()).len(), 2);
    }

    #[test]
    fn allows_on_click_in_client_component() {
        let src = r#"
export default function Page() {
    return <button onClick={() => {}}>X</button>;
}
"#;
        assert!(run(src, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_non_handler_attrs_in_server_component() {
        // `className`, `action`, `href` are all server-safe.
        let src = r#"
export default function Page() {
    return <form action={createPost} className="x"><a href="/">home</a></form>;
}
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }

    #[test]
    fn ignores_custom_attr_starting_with_on_only() {
        // `one` doesn't match `on[A-Z]`.
        let src = r#"
export default function Page() {
    return <div one="1" only="2" />;
}
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }

    #[test]
    fn ignores_unknown_rsc_context() {
        let src = r#"
export default function Page() {
    return <button onClick={() => {}} />;
}
"#;
        assert!(run(src, &FileCtx::default()).is_empty());
    }
}
