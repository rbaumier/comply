//! react-no-class-component-in-server-component backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::file_ctx::RscContext;

const REACT_BASES: &[&str] = &["Component", "PureComponent"];

fn extends_react_component(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "class_heritage" {
            continue;
        }
        let Ok(heritage_text) = child.utf8_text(source) else { continue };
        let Some(rest) = heritage_text.strip_prefix("extends") else { continue };
        let super_name = rest
            .trim()
            .split(|c: char| !c.is_alphanumeric() && c != '.')
            .next()
            .unwrap_or("");
        let base = super_name.rsplit('.').next().unwrap_or(super_name);
        if REACT_BASES.contains(&base) {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["class_declaration"] => |node, source, ctx, diagnostics|
    if ctx.file.rsc_context != RscContext::ServerComponent {
        return;
    }
    if !extends_react_component(node, source) {
        return;
    }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "react-no-class-component-in-server-component".into(),
        message: "Class components don't render on the server. Rewrite this as \
                  a function component or move it to a `\"use client\"` module."
            .into(),
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
    fn flags_class_extends_component_in_server() {
        let src = r#"
class Page extends Component {
    render() { return <div />; }
}
"#;
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn flags_class_extends_react_component_in_server() {
        let src = r#"
class Page extends React.Component {
    render() { return <div />; }
}
"#;
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn flags_class_extends_pure_component_in_server() {
        let src = r#"
class Page extends React.PureComponent {
    render() { return <div />; }
}
"#;
        assert_eq!(run(src, &server_ctx()).len(), 1);
    }

    #[test]
    fn allows_class_extends_component_in_client() {
        let src = r#"
class Page extends Component {
    render() { return <div />; }
}
"#;
        assert!(run(src, &client_ctx()).is_empty());
    }

    #[test]
    fn allows_non_react_class_in_server() {
        let src = r#"
class Logger extends BaseLogger {
    log(msg) {}
}
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }

    #[test]
    fn allows_plain_class_in_server() {
        let src = r#"
class Config {
    constructor() { this.x = 1; }
}
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }

    #[test]
    fn allows_function_component_in_server() {
        let src = r#"
export default function Page() { return <div />; }
"#;
        assert!(run(src, &server_ctx()).is_empty());
    }
}
