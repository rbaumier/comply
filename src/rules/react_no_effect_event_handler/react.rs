//! Flags `useEffect(() => { if (dep) { ... } }, [dep])` — sole `if` testing a
//! dependency variable.

use crate::diagnostic::{Diagnostic, Severity};

fn is_effect_hook(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = node.child_by_field_name("function") else {
        return false;
    };
    match callee.kind() {
        "identifier" => {
            let name = callee.utf8_text(source).ok().unwrap_or("");
            name == "useEffect" || name == "useLayoutEffect"
        }
        "member_expression" => {
            let obj = callee
                .child_by_field_name("object")
                .and_then(|o| o.utf8_text(source).ok());
            let prop = callee
                .child_by_field_name("property")
                .and_then(|p| p.utf8_text(source).ok());
            obj == Some("React") && matches!(prop, Some("useEffect") | Some("useLayoutEffect"))
        }
        _ => false,
    }
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    if !is_effect_hook(node, source) {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    let mut cursor = args.walk();
    let named: Vec<_> = args.children(&mut cursor).filter(|c| c.is_named()).collect();
    if named.len() < 2 {
        return;
    }

    let callback = named[0];
    if callback.kind() != "arrow_function" && callback.kind() != "function_expression" {
        return;
    }

    let deps = named[1];
    if deps.kind() != "array" {
        return;
    }
    let mut dc = deps.walk();
    let dep_names: Vec<&str> = deps
        .children(&mut dc)
        .filter(|c| c.kind() == "identifier")
        .filter_map(|c| c.utf8_text(source).ok())
        .collect();
    if dep_names.is_empty() {
        return;
    }

    let Some(body) = callback.child_by_field_name("body") else { return };
    if body.kind() != "statement_block" {
        return;
    }

    let mut bc = body.walk();
    let stmts: Vec<_> = body.children(&mut bc).filter(|c| c.is_named()).collect();
    if stmts.len() != 1 {
        return;
    }

    let stmt = stmts[0];
    if stmt.kind() != "if_statement" {
        return;
    }

    let Some(condition) = stmt.child_by_field_name("condition") else { return };

    // Unwrap parenthesized_expression if present.
    let test_node = if condition.kind() == "parenthesized_expression" {
        condition.named_child(0).unwrap_or(condition)
    } else {
        condition
    };

    if test_node.kind() != "identifier" {
        return;
    }
    let test_name = test_node.utf8_text(source).ok().unwrap_or("");
    if !dep_names.contains(&test_name) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: node.start_position().row + 1,
        column: node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`useEffect` simulating an event handler — `{test_name}` change should be handled where it is set."
        ),
        severity: Severity::Warning,
        span: None,
    });
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
    fn flags_if_dep_pattern() {
        assert_eq!(
            run(r#"
function App() {
    const [submitted, setSubmitted] = useState(false);
    useEffect(() => {
        if (submitted) { navigate('/success'); }
    }, [submitted]);
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn flags_layout_effect() {
        assert_eq!(
            run(r#"
function App() {
    useLayoutEffect(() => {
        if (ready) { doSomething(); }
    }, [ready]);
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn allows_empty_deps() {
        assert!(
            run(r#"
function App() {
    useEffect(() => {
        if (submitted) { doSomething(); }
    }, []);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_multi_statement_body() {
        assert!(
            run(r#"
function App() {
    useEffect(() => {
        console.log('effect');
        if (submitted) { navigate('/'); }
    }, [submitted]);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_non_dep_condition() {
        assert!(
            run(r#"
function App() {
    useEffect(() => {
        if (otherVar) { doSomething(); }
    }, [submitted]);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_no_deps_array() {
        assert!(
            run(r#"
function App() {
    useEffect(() => {
        if (x) { doSomething(); }
    });
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_non_if_body() {
        assert!(
            run(r#"
function App() {
    useEffect(() => {
        fetchData(dep);
    }, [dep]);
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn flags_react_dot_use_effect() {
        assert_eq!(
            run(r#"
function App() {
    React.useEffect(() => {
        if (submitted) { navigate('/success'); }
    }, [submitted]);
}
"#)
            .len(),
            1
        );
    }
}
