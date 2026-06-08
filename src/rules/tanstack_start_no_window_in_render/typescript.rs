//! Scan exported component render bodies for `window.*` or `document.*`
//! member expressions that are NOT inside `useEffect`/`useLayoutEffect`
//! callbacks or nested functions (event handlers).
//!
//! Accesses guarded by `typeof window !== 'undefined'` /
//! `typeof document !== 'undefined'` (in any enclosing `if` condition) are
//! skipped — the rule's own remediation explicitly allows that pattern.

use crate::diagnostic::{Diagnostic, Severity};

const SAFE_CALLBACK_HOOKS: &[&str] = &[
    "useEffect",
    "useLayoutEffect",
    "useCallback",
    "useMemo",
    "useImperativeHandle",
];

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let mut stack: Vec<tree_sitter::Node> = vec![node];
    while let Some(n) = stack.pop() {
        if is_component_function(n, source) {
            let body = if n.kind() == "variable_declarator" {
                n.child_by_field_name("value")
                    .and_then(|v| v.child_by_field_name("body"))
            } else {
                n.child_by_field_name("body")
            };
            if let Some(body) = body {
                scan_render_body(body, source, ctx.path, diagnostics);
            }
        }
        let mut cursor = n.walk();
        for c in n.children(&mut cursor) {
            stack.push(c);
        }
    }
}

fn is_component_function(n: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    match n.kind() {
        "function_declaration" => name_is_pascal(n.child_by_field_name("name"), source),
        "variable_declarator" => {
            if !name_is_pascal(n.child_by_field_name("name"), source) {
                return false;
            }
            let Some(v) = n.child_by_field_name("value") else {
                return false;
            };
            matches!(
                v.kind(),
                "arrow_function" | "function_expression" | "function"
            )
        }
        _ => false,
    }
}

fn name_is_pascal(name: Option<tree_sitter::Node<'_>>, source: &[u8]) -> bool {
    name.and_then(|n| n.utf8_text(source).ok())
        .and_then(|s| s.chars().next())
        .is_some_and(|c| c.is_ascii_uppercase())
}

fn scan_render_body(
    body: tree_sitter::Node<'_>,
    source: &[u8],
    path: &std::path::Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if let Some(name) = offending_member(n, source)
            && !is_guarded_by_typeof(n, source, name)
        {
            diagnostics.push(Diagnostic::at_node(
                path,
                &n,
                super::META.id,
                format!(
                    "`{name}.*` in render breaks SSR. Read from `{name}` inside a \
                     `useEffect`, or guard with `typeof {name} !== 'undefined'`."
                ),
                Severity::Warning,
            ));
        }

        if is_nested_function(n) {
            continue;
        }

        let mut cursor = n.walk();
        for c in n.children(&mut cursor) {
            if is_safe_callback_hook(c, source) {
                continue;
            }
            stack.push(c);
        }
    }
}

fn is_nested_function(n: tree_sitter::Node<'_>) -> bool {
    matches!(
        n.kind(),
        "arrow_function"
            | "function_expression"
            | "function"
            | "function_declaration"
            | "method_definition"
    )
}

fn is_safe_callback_hook(n: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if n.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = n.child_by_field_name("function") else {
        return false;
    };
    let Ok(name) = callee.utf8_text(source) else {
        return false;
    };
    SAFE_CALLBACK_HOOKS.contains(&name)
}

/// True when `n` is inside an `if_statement` (or surrounding ternary) whose
/// condition contains `typeof <name> !== "undefined"` — string-matching the
/// condition text is enough since the user's own remediation suggests that
/// exact form.
fn is_guarded_by_typeof(n: tree_sitter::Node<'_>, source: &[u8], name: &str) -> bool {
    let needle_dq = format!("typeof {name} !== \"undefined\"");
    let needle_sq = format!("typeof {name} !== 'undefined'");
    let mut cur = n.parent();
    while let Some(p) = cur {
        if matches!(p.kind(), "if_statement" | "ternary_expression") {
            let cond = p
                .child_by_field_name("condition")
                .or_else(|| p.named_child(0));
            if let Some(c) = cond
                && let Ok(t) = c.utf8_text(source)
            {
                let normalized: String = t.split_whitespace().collect::<Vec<_>>().join(" ");
                if normalized.contains(&needle_dq) || normalized.contains(&needle_sq) {
                    return true;
                }
            }
        }
        cur = p.parent();
    }
    false
}

fn offending_member(n: tree_sitter::Node<'_>, source: &[u8]) -> Option<&'static str> {
    if n.kind() != "member_expression" {
        return None;
    }
    let obj = n.child_by_field_name("object")?;
    if obj.kind() != "identifier" {
        return None;
    }
    let name = obj.utf8_text(source).ok()?;
    match name {
        "window" => Some("window"),
        "document" => Some("document"),
        _ => None,
    }
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_window_in_component() {
        let src = "function Page() { const w = window.innerWidth; return w; }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_document_in_component() {
        let src = "const Page = () => { const el = document.body; return el; };";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_in_use_effect() {
        let src = "function Page() { useEffect(() => { const w = window.innerWidth; }, []); return null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_in_event_handler() {
        let src = "function Page() { const onClick = () => { document.title = 'x'; }; return <button onClick={onClick}/>; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_component() {
        let src = "function helper() { return window.location.href; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_typeof_guarded_window() {
        let src = "function Page() { \
                   if (typeof window !== 'undefined') { const w = window.innerWidth; } \
                   return null; }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_typeof_guarded_document_double_quoted() {
        let src = "function Page() { \
                   if (typeof document !== \"undefined\") { const t = document.title; } \
                   return null; }";
        assert!(run(src).is_empty());
    }
}
