use crate::diagnostic::{Diagnostic, Severity};

/// A React component is a function whose name starts with an uppercase letter
/// (PascalCase). Anything else — lowercase functions, methods, object members,
/// Zod maps — counts as a non-React context for this rule.
fn is_react_component_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

crate::ast_check! { on ["call_expression"] prefilter = ["useTranslation"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.utf8_text(source).unwrap_or("") != "useTranslation" { return; }

    // Walk upward to find the enclosing function/component.
    let mut cursor = node.parent();
    let mut in_react_component = false;
    while let Some(parent) = cursor {
        match parent.kind() {
            "function_declaration" => {
                let name = parent
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .unwrap_or("");
                in_react_component = is_react_component_name(name);
                break;
            }
            "variable_declarator" => {
                let value = parent.child_by_field_name("value").map(|v| v.kind());
                if matches!(value, Some("arrow_function") | Some("function_expression")) {
                    let name = parent
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .unwrap_or("");
                    in_react_component = is_react_component_name(name);
                    break;
                }
                cursor = parent.parent();
            }
            "method_definition" => {
                break;
            }
            _ => cursor = parent.parent(),
        }
    }
    if in_react_component { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "useTranslation() must only run inside a React component. Use the `i18n.t()` singleton here.".into(),
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
    fn flags_outside_component() {
        let src = "function head() { const { t } = useTranslation(); return t('x'); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_inside_component() {
        let src = "function MyComponent() { const { t } = useTranslation(); return null; }";
        assert!(run(src).is_empty());
    }
}
