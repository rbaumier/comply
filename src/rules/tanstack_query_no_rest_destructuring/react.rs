//! Flags `const { data, ...rest } = useQuery(...)` (or `useInfiniteQuery`,
//! `useMutation`). Rest destructuring touches every field on the result, so
//! the component re-renders on any internal state transition.

use crate::diagnostic::{Diagnostic, Severity};

const QUERY_HOOKS: &[&str] = &[
    "useQuery",
    "useInfiniteQuery",
    "useSuspenseQuery",
    "useSuspenseInfiniteQuery",
    "useMutation",
    "useQueries",
];

fn callee_name<'a>(call: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let callee = call.child_by_field_name("function")?;
    match callee.kind() {
        "identifier" => callee.utf8_text(source).ok(),
        _ => None,
    }
}

fn pattern_has_rest(pattern: tree_sitter::Node) -> bool {
    if pattern.kind() != "object_pattern" {
        return false;
    }
    let mut cursor = pattern.walk();
    pattern
        .children(&mut cursor)
        .any(|c| c.kind() == "rest_pattern")
}

crate::ast_check! { on ["variable_declarator"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    if !pattern_has_rest(name_node) {
        return;
    }

    let Some(init) = node.child_by_field_name("value") else { return };
    if init.kind() != "call_expression" {
        return;
    }

    let Some(name) = callee_name(init, source) else { return };
    if !QUERY_HOOKS.contains(&name) {
        return;
    }

    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: name_node.start_position().row + 1,
        column: name_node.start_position().column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Rest destructuring on `{name}()` result — destructure only the fields you actually use."
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
    fn flags_use_query_rest() {
        let diags = run(r#"
function Users() {
    const { data, ...rest } = useQuery({ queryKey: ['u'], queryFn });
    return <div />;
}
"#);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("useQuery"));
    }

    #[test]
    fn flags_use_infinite_query_rest() {
        assert_eq!(
            run(r#"
function Feed() {
    const { data, ...rest } = useInfiniteQuery(opts);
    return <div />;
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn flags_use_mutation_rest() {
        assert_eq!(
            run(r#"
function Form() {
    const { mutate, ...rest } = useMutation({ mutationFn });
    return <div />;
}
"#)
            .len(),
            1
        );
    }

    #[test]
    fn allows_named_destructuring() {
        assert!(
            run(r#"
function Users() {
    const { data, isLoading, error } = useQuery({ queryKey: ['u'], queryFn });
    return <div />;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_full_assignment() {
        assert!(
            run(r#"
function Users() {
    const query = useQuery({ queryKey: ['u'], queryFn });
    return <div />;
}
"#)
            .is_empty()
        );
    }

    #[test]
    fn allows_rest_on_non_query_call() {
        assert!(
            run(r#"
function App() {
    const { foo, ...rest } = somethingElse();
    return <div />;
}
"#)
            .is_empty()
        );
    }
}
