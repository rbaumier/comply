//! better-auth-expo-no-cookie-auth — require `expoClient()` in React Native/Expo files.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "program" {
        return;
    }

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

    if text.contains("expoClient") {
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
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
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
    fn ignores_web_file() {
        let src = r#"
            import { createAuthClient } from "better-auth/react";
            export const authClient = createAuthClient({});
        "#;
        assert!(run(src).is_empty());
    }
}
