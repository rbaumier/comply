//! Flags a `type_alias_declaration` whose body is a `conditional_type` or
//! `mapped_type` that references the alias itself, when the alias has no
//! "depth-like" parameter. Depth parameters are detected by name pattern
//! (`D`, `Depth`, `N`, `Count`, or constrained with `extends number`).

use crate::diagnostic::{Diagnostic, Severity};

fn references_name(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    if node.kind() == "type_identifier" {
        let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
        return text == name;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if references_name(child, source, name) {
            return true;
        }
    }
    false
}

fn has_depth_parameter(type_params: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = type_params.walk();
    for tp in type_params.named_children(&mut cursor) {
        if tp.kind() != "type_parameter" {
            continue;
        }
        let text = std::str::from_utf8(&source[tp.byte_range()]).unwrap_or("");
        // name hints
        if text.contains("Depth") || text.contains("Count") {
            return true;
        }
        // numeric constraint
        if text.contains("extends number") || text.contains("extends 0") {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["type_alias_declaration"] => |node, source, ctx, diagnostics|
    let Some(name_node) = node.child_by_field_name("name") else { return };
    let name = std::str::from_utf8(&source[name_node.byte_range()]).unwrap_or("").to_string();
    if name.is_empty() {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else { return };

    // Must be a conditional or mapped type at the top level.
    let mut inner = value;
    while inner.kind() == "parenthesized_type" {
        let Some(c) = inner.named_child(0) else { break };
        inner = c;
    }
    if !matches!(inner.kind(), "conditional_type" | "mapped_type") {
        return;
    }

    // Must reference itself.
    if !references_name(value, source, &name) {
        return;
    }

    // Must lack a depth parameter.
    if let Some(tp) = node.child_by_field_name("type_parameters")
        && has_depth_parameter(tp, source)
    {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("Recursive type `{name}` has no depth parameter; add one to bound recursion."),
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_recursive_conditional_without_depth() {
        let src = "type Flatten<T> = T extends Array<infer U> ? Flatten<U> : T;";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_recursive_with_depth() {
        let src =
            "type Flatten<T, Depth extends number = 5> = Depth extends 0 ? T : Flatten<T, 0>;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_recursive_conditional() {
        let src = "type IsString<T> = T extends string ? true : false;";
        assert!(run(src).is_empty());
    }
}
