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
        let text = ctx.source;

        let is_native = text.contains("from \"react-native\"")
            || text.contains("from 'react-native'")
            || text.contains("from \"expo\"")
            || text.contains("from 'expo'")
            || text.contains("from \"expo-router\"")
            || text.contains("from 'expo-router'");

        if !is_native {
            return Vec::new();
        }

        if !text.contains("createAuthClient") {
            return Vec::new();
        }

        if text.contains("expoClient(") {
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
