//! better-auth-expo-no-cookie-auth OXC backend — require `expoClient()` in React Native/Expo files.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["createAuthClient"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {

        let is_native = ctx.source_contains("from \"react-native\"")
            || ctx.source_contains("from 'react-native'")
            || ctx.source_contains("from \"expo\"")
            || ctx.source_contains("from 'expo'")
            || ctx.source_contains("from \"expo-router\"")
            || ctx.source_contains("from 'expo-router'");

        if !is_native {
            return Vec::new();
        }

        if !ctx.source_contains("createAuthClient") {
            return Vec::new();
        }

        if ctx.source_contains("expoClient(") {
            return Vec::new();
        }

        vec![Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: super::META.id.into(),
            message: "React Native/Expo file uses cookie-based `createAuthClient` — add `expoClient()` from `@better-auth/expo/client`.".into(),
            severity: Severity::Error,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;
    use super::Check;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
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
