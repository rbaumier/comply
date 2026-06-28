//! nuxt-no-v-html-in-server OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::source_contains;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_nuxt_or_vue_source(src: &str) -> bool {
    source_contains(src, "#imports")
        || source_contains(src, "nuxt/app")
        || source_contains(src, "#app")
        || source_contains(src, "defineNuxtPlugin")
        || source_contains(src, "defineNuxtRouteMiddleware")
        || source_contains(src, "useNuxtApp")
        || source_contains(src, "<template")
        || source_contains(src, "defineComponent")
}

/// Byte-offset spans (`[start, end)`) of every `StringLiteral` and
/// `TemplateLiteral` node in the file. Text inside one of these is treated as
/// source-code-as-data (e.g. a quoted Vue SFC source string in a server-side
/// component registry/codegen file), not a live template directive.
///
/// A `v-html` inside one of these spans is consequently never flagged. The one
/// exception this collapses is a string handed to Vue's runtime template
/// compiler (`defineComponent({ template: '<div v-html=...>' })`), which is
/// also data here yet is compiled at runtime; such runtime-compiled string
/// templates are uncommon in Nuxt/Nitro server code (Nuxt compiles SFC
/// templates at build time) and are the accepted cost of using string-literal
/// containment as the structural discriminator. A live `v-html` written as a
/// real attribute (`<div v-html={x} />`, `<div v-html="x" />`) is outside every
/// span and still flagged.
fn string_literal_spans(semantic: &oxc_semantic::Semantic) -> Vec<(u32, u32)> {
    semantic
        .nodes()
        .iter()
        .filter_map(|node| match node.kind() {
            AstKind::StringLiteral(lit) => Some((lit.span.start, lit.span.end)),
            AstKind::TemplateLiteral(tpl) => Some((tpl.span.start, tpl.span.end)),
            _ => None,
        })
        .collect()
}

/// True when byte offset `abs` lies inside any string-literal/template-literal
/// span — i.e. the match is the *content* of a quoted string, not live code.
fn offset_in_string_literal(abs: usize, spans: &[(u32, u32)]) -> bool {
    let abs = abs as u32;
    spans.iter().any(|&(start, end)| abs >= start && abs < end)
}

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["v-html"])
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !is_nuxt_or_vue_source(src) {
            return Vec::new();
        }
        let sanitized = ctx.source_contains("DOMPurify")
            || ctx.source_contains("sanitize(")
            || ctx.source_contains("sanitizeHtml(")
            || ctx.source_contains("purify(");

        // Collected once per file (not per match) so the per-`v-html` gate is a
        // cheap span scan.
        let literal_spans = string_literal_spans(semantic);

        let mut diagnostics = Vec::new();
        let mut start = 0;
        while let Some(pos) = src[start..].find("v-html") {
            let abs = start + pos;
            let prev = if abs == 0 { ' ' } else { src.as_bytes()[abs - 1] as char };
            if prev.is_alphanumeric() || prev == '_' || prev == '-' {
                start = abs + 6;
                continue;
            }
            if offset_in_string_literal(abs, &literal_spans) {
                start = abs + 6;
                continue;
            }
            if !sanitized {
                let line = src[..abs].matches('\n').count() + 1;
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column: 1,
                    rule_id: super::META.id.into(),
                    message: "`v-html` without DOMPurify/sanitize — XSS risk in SSR. Sanitize the value or render as components.".into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
            start = abs + 6;
        }
        diagnostics
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
    ) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_helpers::run_rule;

    /// Regression for #6501: a server-side registry stores Vue SFC source as
    /// quoted JS strings. `<template>` and `v-html` appear only inside string
    /// literals (data), never as live directives, so the file must not flag.
    #[test]
    fn ignores_v_html_inside_sfc_source_string() {
        let src = r#"
const components = [
  {
    fileName: "Card/Card.vue",
    fileContent:
      '<template>\n  <div v-html="content" />\n</template>\n<script setup></script>',
  },
];
export default components;
"#;
        assert!(
            run_rule(&Check, src, "server/utils/comp.ts").is_empty(),
            "v-html inside a quoted SFC-source string is data, not a live directive"
        );
    }

    /// The same registry shape using a template literal to hold the SFC source.
    #[test]
    fn ignores_v_html_inside_template_literal_source() {
        let src = "
const tpl = `<template>\n  <div v-html=\"content\" />\n</template>`;
export const registry = { 'Card.vue': tpl };
";
        assert!(
            run_rule(&Check, src, "server/utils/registry.ts").is_empty(),
            "v-html inside a template-literal SFC source is data, not a live directive"
        );
    }

    /// Negative control: a live `v-html` JSX attribute (outside any string
    /// literal) in a Vue-source file is a real unsanitized binding and flags.
    #[test]
    fn flags_live_v_html_jsx_attribute() {
        let src = r#"
import { defineComponent } from "vue";
export default defineComponent({
  render: (content: string) => <div v-html={content} />,
});
"#;
        assert_eq!(
            run_rule(&Check, src, "server/render.tsx").len(),
            1,
            "a live v-html JSX attribute outside any string literal must still flag"
        );
    }

    /// A live `v-html` attribute whose value happens to be a string literal:
    /// the attribute *name* is outside the value's span, so it still flags.
    #[test]
    fn flags_live_v_html_with_string_value() {
        let src = r#"
import { defineComponent } from "vue";
export default defineComponent({
  render: () => <div v-html="rawHtml" />,
});
"#;
        assert_eq!(
            run_rule(&Check, src, "server/render.tsx").len(),
            1,
            "a real v-html attribute with a static string value must still flag"
        );
    }

    /// Mixed file: a data `v-html` inside a string and a live `v-html` JSX
    /// attribute coexist. The gate is per-match, so only the live one flags —
    /// the data string does not shadow the sibling directive, nor vice versa.
    #[test]
    fn flags_only_the_live_v_html_in_mixed_file() {
        let src = r#"
import { defineComponent } from "vue";
const sfcSource = '<template><div v-html="content" /></template>';
export default defineComponent({
  render: (content: string) => <div v-html={content} />,
});
"#;
        assert_eq!(
            run_rule(&Check, src, "server/render.tsx").len(),
            1,
            "only the live v-html flags; the v-html inside the SFC-source string is data"
        );
    }

    /// A sanitized live `v-html` binding stays quiet (existing behavior).
    #[test]
    fn allows_sanitized_v_html() {
        let src = r#"
import { defineComponent } from "vue";
import DOMPurify from "dompurify";
export default defineComponent({
  render: (content: string) => <div v-html={DOMPurify.sanitize(content)} />,
});
"#;
        assert!(
            run_rule(&Check, src, "server/render.tsx").is_empty(),
            "a DOMPurify-sanitized v-html binding is safe"
        );
    }
}
