//! Flags `style={{ ... }}` on JSX elements (object literal value).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["jsx_attribute"] => |node, source, ctx, diagnostics|
    if !ctx.project.has_framework("react-native") { return; }
    let Some(name) = crate::rules::jsx::jsx_attribute_name(node, source) else { return };
    if name != "style" { return; }
    let Some(value) = crate::rules::jsx::jsx_attribute_value(node) else { return };
    if value.kind() != "jsx_expression" { return; }
    let mut cursor = value.walk();
    for child in value.children(&mut cursor) {
        match child.kind() {
            "{" | "}" => continue,
            "object" => {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &child,
                    super::META.id,
                    "Inline style object allocates on every render — use `StyleSheet.create` or `useMemo`.".into(),
                    Severity::Warning,
                ));
                return;
            }
            _ => return,
        }
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
        crate::rules::test_helpers::run_rule_with_ctx(&Check, s, "t.tsx", &crate::project::ProjectCtx::for_test_with_framework("react-native"), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_inline_style() {
        let src = "const x = <View style={{ padding: 8 }} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_stylesheet_reference() {
        let src = "const x = <View style={styles.container} />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_array_style_with_refs() {
        let src = "const x = <View style={[styles.a, styles.b]} />;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_non_react_native_projects() {
        let src = "const x = <div style={{ padding: 8 }} />;";
        assert!(crate::rules::test_helpers::run_rule(&Check, src, "t.tsx").is_empty());
    }
}
