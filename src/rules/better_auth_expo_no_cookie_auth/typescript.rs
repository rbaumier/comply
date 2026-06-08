//! better-auth-expo-no-cookie-auth — require `expoClient()` in React Native/Expo files.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] prefilter = ["createAuthClient"] => |node, source, ctx, diagnostics|
    let text = std::str::from_utf8(source).unwrap_or("");

    let is_native = text.contains("from \"react-native\"")
        || text.contains("from 'react-native'")
        || text.contains("from \"expo\"")
        || text.contains("from 'expo'")
        || text.contains("from \"expo-router\"")
        || text.contains("from 'expo-router'");

    if !is_native {
        return;
    }

    if !text.contains("createAuthClient") {
        return;
    }

    // Require an actual call `expoClient(` — bare identifiers in comments or
    // unused imports do not satisfy the rule.
    if text.contains("expoClient(") {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "React Native/Expo file uses cookie-based `createAuthClient` — add `expoClient()` from `@better-auth/expo/client`.".into(),
        Severity::Error,
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_expo_without_expoclient() {
        let src = r#"
            import { View } from "react-native";
            import { createAuthClient } from "better-auth/react";
            export const authClient = createAuthClient({});
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_with_expoclient() {
        let src = r#"
            import { View } from "react-native";
            import { createAuthClient } from "better-auth/react";
            import { expoClient } from "@better-auth/expo/client";
            export const authClient = createAuthClient({ plugins: [expoClient()] });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_when_expoclient_only_in_comment() {
        let src = r#"
            import { View } from "react-native";
            import { createAuthClient } from "better-auth/react";
            // TODO: add expoClient plugin
            export const authClient = createAuthClient({});
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_web_file() {
        let src = r#"
            import { createAuthClient } from "better-auth/react";
            export const authClient = createAuthClient({});
        "#;
        assert!(run(src).is_empty());
    }
}
