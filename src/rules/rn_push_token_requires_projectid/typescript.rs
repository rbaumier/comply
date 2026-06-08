//! Flags `getExpoPushTokenAsync()` calls whose argument object lacks `projectId`.

use crate::diagnostic::{Diagnostic, Severity};

fn object_has_project_id(obj: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if obj.kind() != "object" {
        return false;
    }
    let mut cursor = obj.walk();
    for child in obj.named_children(&mut cursor) {
        // Look at pair / shorthand_property_identifier entries.
        match child.kind() {
            "pair" => {
                if let Some(key) = child.child_by_field_name("key")
                    && let Ok(k) = key.utf8_text(source)
                    && k.trim_matches(|c| c == '"' || c == '\'') == "projectId"
                {
                    return true;
                }
            }
            "shorthand_property_identifier" => {
                if let Ok(k) = child.utf8_text(source)
                    && k == "projectId"
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    let Ok(name) = func.utf8_text(source) else { return };
    if !name.ends_with("getExpoPushTokenAsync") { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let first = args.named_child(0);
    let has_project_id = match first {
        Some(n) if n.kind() == "object" => object_has_project_id(n, source),
        _ => false,
    };
    if has_project_id { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`getExpoPushTokenAsync` must be called with `{ projectId }` — required by EAS.".into(),
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_no_args() {
        let src = "await Notifications.getExpoPushTokenAsync();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_empty_object() {
        let src = "await Notifications.getExpoPushTokenAsync({});";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_project_id() {
        let src = "await Notifications.getExpoPushTokenAsync({ projectId: 'abc' });";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_with_shorthand_project_id() {
        let src = "await Notifications.getExpoPushTokenAsync({ projectId });";
        assert!(run(src).is_empty());
    }
}
