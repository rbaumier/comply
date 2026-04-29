//! Flags imports from `@react-navigation/stack` and calls to `createStackNavigator`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["import_statement", "call_expression"] prefilter = ["@react-navigation/stack", "createStackNavigator"] => |node, source, ctx, diagnostics|
match node.kind() {
        "import_statement" => {
            let Some(src_node) = node.child_by_field_name("source") else { return };
            let Ok(raw) = src_node.utf8_text(source) else { return };
            let spec = raw.trim_matches(|c| c == '"' || c == '\'');
            if spec == "@react-navigation/stack" {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    "Import from `@react-navigation/stack` is forbidden — use Expo Router.".into(),
                    Severity::Warning,
                ));
            }
        }
        "call_expression" => {
            let Some(func) = node.child_by_field_name("function") else { return };
            let Ok(name) = func.utf8_text(source) else { return };
            if name == "createStackNavigator" {
                diagnostics.push(Diagnostic::at_node(
                    ctx.path,
                    &node,
                    super::META.id,
                    "`createStackNavigator` is forbidden — migrate to Expo Router.".into(),
                    Severity::Warning,
                ));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
    }

    #[test]
    fn flags_import_from_stack() {
        let src = "import { createStackNavigator } from '@react-navigation/stack';";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_create_stack_call() {
        let src = "const Stack = createStackNavigator();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_expo_router() {
        let src = "import { Stack } from 'expo-router';";
        assert!(run(src).is_empty());
    }
}
