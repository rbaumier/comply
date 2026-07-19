//! vue-no-target-blank — Vue text backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::html_rel_helpers::rel_is_safe;
use crate::rules::vue_template_helpers::{
    attr_value, collect_attr_names, extract_elements, is_vue_file,
};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["_blank"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for elem in extract_elements(ctx.source) {
            // Reverse-tabnabbing via `window.opener` is a native-anchor concern:
            // only a native `<a>`/`<area>` navigates the browser directly. A Vue
            // component takes `target` as a prop whose rendered DOM the rule
            // cannot analyze (`<SettingsItem>`), and framework link components
            // inject `rel` on target by construction (`<NuxtLink>`/`<nuxt-link>`).
            // Native HTML tags are lowercase; a PascalCase or hyphenated tag is a
            // component, so anything but a native anchor is skipped.
            if !matches!(elem.tag, "a" | "area") {
                continue;
            }
            let target = attr_value(elem.attrs, "target");
            if target != Some("_blank") {
                continue;
            }
            // Reverse-tabnabbing needs a document to open: only an anchor that
            // navigates can leak `window.opener`. Require a navigable href —
            // static `href` or a bound `:href`/`v-bind:href` — so a
            // `target="_blank"` element with no href (e.g. a popover trigger)
            // is not flagged.
            let has_href = collect_attr_names(elem.attrs)
                .iter()
                .any(|name| *name == "href" || name.ends_with(":href"));
            if !has_href {
                continue;
            }
            let has_safe_rel = attr_value(elem.attrs, "rel").is_some_and(rel_is_safe);
            if !has_safe_rel {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: elem.line,
                    column: 1,
                    rule_id: "vue-no-target-blank".into(),
                    message: "`target=\"_blank\"` without `rel=\"noopener\"` (or `noreferrer`) \
                              allows the opened page to access `window.opener`. \
                              Add `rel=\"noopener\"`."
                        .into(),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_vue_template() {
        let source =
            "<template>\n  <a href=\"https://example.com\" target=\"_blank\">link</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn allows_with_noreferrer() {
        let source = "<template>\n  <a href=\"https://example.com\" target=\"_blank\" rel=\"noreferrer\">link</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_with_noopener() {
        // `rel="noopener"` alone severs `window.opener` (issue #6939, real snippet
        // uses a bound `:href`).
        let source = "<template>\n  <a :href=\"item.docsUrl\" target=\"_blank\" rel=\"noopener\">link</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn allows_with_noopener_noreferrer() {
        let source = "<template>\n  <a href=\"https://example.com\" target=\"_blank\" rel=\"noopener noreferrer\">link</a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_unrelated_rel_token() {
        let source = "<template>\n  <a href=\"https://example.com\" target=\"_blank\" rel=\"nofollow\">link</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn flags_substring_trap() {
        // `notnoopener` merely contains `noopener` as a substring; it is not the token.
        let source = "<template>\n  <a href=\"https://example.com\" target=\"_blank\" rel=\"notnoopener\">link</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn ignores_target_blank_without_href() {
        // A non-navigating anchor (no href) opens no document, so there is no
        // `window.opener` to leak — issue #7517 (an `el-popover` reference).
        let source = "<template>\n  <a slot=\"reference\" target=\"_blank\">\n    <el-button>QQ</el-button>\n  </a>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn flags_bound_href_without_safe_rel() {
        // A bound `:href` still navigates, so an unsafe `rel` is a real risk.
        let source = "<template>\n  <a :href=\"url\" target=\"_blank\">link</a>\n</template>";
        assert_eq!(run(source).len(), 1);
    }

    #[test]
    fn ignores_nuxt_link_with_static_href() {
        // #7556: `<NuxtLink>` is a Vue component that auto-injects
        // `rel="noopener noreferrer"` when a target is set; the rule cannot see
        // its rendered anchor, so it must not be flagged.
        let source = "<template>\n  <NuxtLink href=\"https://nuxtlabs.com\" target=\"_blank\">x</NuxtLink>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_nuxt_link_with_bound_to() {
        // #7556: a bound `:to` on `<NuxtLink target="_blank">` renders a safe anchor.
        let source =
            "<template>\n  <NuxtLink :to=\"url\" target=\"_blank\">x</NuxtLink>\n</template>";
        assert!(run(source).is_empty());
    }

    #[test]
    fn ignores_custom_component_with_target_prop() {
        // #7556: `<SettingsItem>` is a PascalCase component; `target` is a prop
        // whose rendered DOM the rule cannot analyze, not a native anchor attribute.
        let source =
            "<template>\n  <SettingsItem :to=\"url\" target=\"_blank\" />\n</template>";
        assert!(run(source).is_empty());
    }
}
