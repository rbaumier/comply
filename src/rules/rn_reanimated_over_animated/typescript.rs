//! Flags `Animated.timing` / `Animated.Value` member expressions and imports
//! of `Animated` from `react-native`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement", "member_expression"] => |node, source, ctx, diagnostics|
match node.kind() {
        "import_statement" => {
            let Some(src_node) = node.child_by_field_name("source") else { return };
            let Ok(raw) = src_node.utf8_text(source) else { return };
            let spec = raw.trim_matches(|c| c == '"' || c == '\'');
            if spec != "react-native" { return; }
            let Ok(full) = node.utf8_text(source) else { return };
            for token in full.split(|c: char| !c.is_alphanumeric() && c != '_') {
                if token == "Animated" {
                    diagnostics.push(Diagnostic::at_node(
                        ctx.path,
                        &node,
                        super::META.id,
                        "Importing `Animated` from 'react-native' — use 'react-native-reanimated' instead.".into(),
                        Severity::Warning,
                    ));
                    return;
                }
            }
        }
        "member_expression" => {
            let Some(obj) = node.child_by_field_name("object") else { return };
            let Ok(obj_text) = obj.utf8_text(source) else { return };
            if obj_text != "Animated" { return; }
            let Some(prop) = node.child_by_field_name("property") else { return };
            let Ok(prop_name) = prop.utf8_text(source) else { return };
            if prop_name != "timing" && prop_name != "Value" { return; }
            diagnostics.push(Diagnostic::at_node(
                ctx.path,
                &node,
                super::META.id,
                format!("`Animated.{prop_name}` is the legacy JS-thread API — use react-native-reanimated."),
                Severity::Warning,
            ));
        }
        _ => {}
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
        crate::rules::test_helpers::run_rule(&Check, s, "t.tsx")
    }

    #[test]
    fn flags_animated_timing() {
        let src = "Animated.timing(val, { toValue: 1 }).start();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_animated_value() {
        let src = "const v = new Animated.Value(0);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_animated_import() {
        let src = "import { Animated } from 'react-native';";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_reanimated() {
        let src = "import { useSharedValue, withTiming } from 'react-native-reanimated';";
        assert!(run(src).is_empty());
    }
}
