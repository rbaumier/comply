//! regex-no-stateful-global TypeScript / JavaScript / TSX backend.
//!
//! Visits tree-sitter `regex` nodes only. A regex literal carrying the
//! `g` flag is flagged when it is bound to a `const` / `let` / `var`
//! whose binding is later used as the receiver of `.test(...)` or
//! `.exec(...)`. The `g` flag makes these methods stateful via
//! `lastIndex`, producing alternating true/false results on repeated
//! calls against the same regex object.
//!
//! Gating by AST eliminates the false-positive class from the previous
//! TextCheck (which matched regex-like substrings inside Tailwind
//! classes, URLs and scoped import paths such as `"@scope/pkg"`).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::regex_ast::pattern_and_flags;

/// Walk up from a `regex` node to the enclosing `variable_declarator`
/// and return its binding identifier text. Returns `None` if the regex
/// is not directly assigned to a simple name — e.g. it sits inside a
/// call argument, an object property, or a destructuring pattern.
fn enclosing_simple_binding<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    let mut current = node.parent()?;
    // Tolerate a single `parenthesized_expression` between the regex
    // and its declarator: `const re = (/foo/g);`.
    while current.kind() == "parenthesized_expression" {
        current = current.parent()?;
    }
    if current.kind() != "variable_declarator" {
        return None;
    }
    let name_node = current.child_by_field_name("name")?;
    // Only simple identifier bindings. Destructuring patterns
    // (`const [r] = [/foo/g]`) are rare for regex and not worth
    // supporting here.
    if name_node.kind() != "identifier" {
        return None;
    }
    name_node.utf8_text(source).ok()
}

/// Walk to the top of the AST from `node` and return the program root.
fn root_of(node: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        cur = parent;
    }
    cur
}

/// True if `root` contains a `call_expression` whose callee is a
/// `member_expression` with object text == `var_name` and property in
/// {`test`, `exec`}. Iterative cursor walk — no recursion, so deeply
/// nested sources cannot blow the stack.
fn has_stateful_usage(root: tree_sitter::Node<'_>, source: &[u8], var_name: &str) -> bool {
    let mut cursor = root.walk();
    'outer: loop {
        let node = cursor.node();
        let bad = node.is_error() || node.is_missing();
        if !bad {
            if node.kind() == "call_expression"
                && let Some(func) = node.child_by_field_name("function")
                && func.kind() == "member_expression"
                && let Some(obj) = func.child_by_field_name("object")
                && obj.kind() == "identifier"
                && let Ok(obj_name) = obj.utf8_text(source)
                && obj_name == var_name
                && let Some(prop) = func.child_by_field_name("property")
                && let Ok(prop_name) = prop.utf8_text(source)
                && matches!(prop_name, "test" | "exec")
            {
                return true;
            }
            if cursor.goto_first_child() {
                continue;
            }
        }
        loop {
            if cursor.goto_next_sibling() {
                continue 'outer;
            }
            if !cursor.goto_parent() {
                return false;
            }
        }
    }
}

crate::ast_check! { on ["regex"] => |node, source, ctx, diagnostics|
    let Some((_pattern, flags)) = pattern_and_flags(&node, source) else { return };
    if !flags.contains('g') {
        return;
    }
    let Some(var_name) = enclosing_simple_binding(node, source) else { return };
    let root = root_of(node);
    if !has_stateful_usage(root, source, var_name) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        "regex-no-stateful-global",
        format!(
            "Regex `{var_name}` has the `g` flag and is used with `.test()`/`.exec()` \u{2014} `lastIndex` is stateful and causes subtle bugs."
        ),
        Severity::Warning,
    ));
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_global_regex_with_test() {
        let src = "const re = /foo/g;\nif (re.test(str)) {}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("lastIndex"));
    }

    #[test]
    fn flags_global_regex_with_exec() {
        let src = "const re = /bar/gi;\nconst m = re.exec(input);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_global_regex_without_test_exec() {
        let src = "const re = /foo/g;\nconst result = str.match(re);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_non_global_regex_with_test() {
        let src = "const re = /foo/i;\nif (re.test(str)) {}";
        assert!(run_on(src).is_empty());
    }

    // --- Regression tests for the TextCheck false-positive class. ---

    #[test]
    fn ignores_tailwind_arbitrary_value_in_string() {
        let src = r#"const x = "has-[>svg]:grid-cols-[auto_1fr]";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_url_in_string() {
        let src = r#"const u = "http://example.com/a/b";"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_scoped_import_path() {
        let src = r#"import X from "@tanstack/react-query";"#;
        assert!(run_on(src).is_empty());
    }
}
