//! Detects `renderXxx()` calls inside JSX expression containers.
//! Walks the full subtree of each `jsx_expression` node.

use crate::diagnostic::{Diagnostic, Severity};

const ALLOWED_RENDER_FNS: &[&str] = &[
    "renderToString",
    "renderToStaticMarkup",
    "renderToPipeableStream",
    "renderToReadableStream",
    "renderToStaticNodeStream",
    "renderToNodeStream",
    "renderHook",
];

fn is_render_call_name(name: &str) -> bool {
    if ALLOWED_RENDER_FNS.contains(&name) {
        return false;
    }
    if let Some(rest) = name.strip_prefix("render") {
        rest.starts_with(|c: char| c.is_ascii_uppercase())
    } else {
        false
    }
}

fn find_render_calls(
    node: tree_sitter::Node,
    source: &[u8],
    ctx: &crate::rules::backend::CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if node.kind() == "call_expression" {
        if let Some(callee) = node.child_by_field_name("function") {
            let callee_name = match callee.kind() {
                "identifier" => callee.utf8_text(source).ok(),
                "member_expression" => {
                    let obj = callee.child_by_field_name("object");
                    let is_this = obj
                        .and_then(|o| o.utf8_text(source).ok())
                        .is_some_and(|t| t == "this" || t == "self");
                    if !is_this {
                        None
                    } else {
                        callee
                            .child_by_field_name("property")
                            .and_then(|p| p.utf8_text(source).ok())
                    }
                }
                _ => None,
            };

            if let Some(name) = callee_name {
                if is_render_call_name(name) {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: node.start_position().row + 1,
                        column: node.start_position().column + 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Inline render function `{name}()` — extract to a component for proper reconciliation."
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                    return;
                }
            }
        }
    }

    // Don't descend into nested JSX — those have their own jsx_expression nodes.
    if node.kind() == "jsx_element" || node.kind() == "jsx_self_closing_element" {
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_render_calls(child, source, ctx, diagnostics);
    }
}

crate::ast_check! { on ["jsx_expression"] => |node, source, ctx, diagnostics|
    find_render_calls(node, source, ctx, diagnostics);
}


#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_inline_render_call() {
        let diags = run(r#"
function App() {
    function renderHeader() {
        return <header>Title</header>;
    }
    return <div>{renderHeader()}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("renderHeader"));
    }

    #[test]
    fn flags_this_member_render_call() {
        let diags = run(r#"
class App extends React.Component {
    render() {
        return <div>{this.renderFooter()}</div>;
    }
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("renderFooter"));
    }

    #[test]
    fn flags_conditional_render_call() {
        let diags = run(r#"
function App({ showHeader }) {
    return <div>{showHeader && renderHeader()}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_ternary_render_call() {
        let diags = run(r#"
function App({ show }) {
    return <div>{show ? renderContent() : null}</div>;
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_map_render_call() {
        let diags = run(r#"
function App({ items }) {
    return <ul>{items.map(item => renderItem(item))}</ul>;
}
"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_component_jsx() {
        assert!(
            run(r#"
function Header() {
    return <header>Title</header>;
}
function App() {
    return <div><Header /></div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_non_render_call() {
        assert!(
            run(r#"
function App() {
    return <div>{getData()}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_render_lowercase_suffix() {
        assert!(
            run(r#"
function App() {
    return <div>{rendering()}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_render_to_string() {
        assert!(
            run(r#"
function App() {
    return <pre>{renderToString(<Inner/>)}</pre>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_props_render_method() {
        assert!(
            run(r#"
function App(props) {
    return <div>{props.renderHeader()}</div>;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn flags_multiple_render_calls() {
        let diags = run(r#"
function App() {
    return (
        <div>
            {renderHeader()}
            {renderBody()}
            {renderFooter()}
        </div>
    );
}
"#);
        assert_eq!(diags.len(), 3);
    }
}
