//! filename-naming-convention — Vue backend (PascalCase or kebab-case).
//!
//! A leading `_`/`__` private-prefix is stripped before the case check, so a
//! private/internal SFC (`_NavigationMenu.vue`) is validated by its remainder —
//! mirroring the TS text backend's `strip_private_prefix`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn is_pascal_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_uppercase() {
        return false;
    }
    let mut has_lower = false;
    for &b in bytes.iter().skip(1) {
        if b.is_ascii_lowercase() || b.is_ascii_digit() {
            has_lower = true;
        } else if b.is_ascii_uppercase() {
            // OK
        } else {
            return false;
        }
    }
    // A single uppercase letter (`P`, `A`) is valid PascalCase by definition —
    // Nuxt Content names prose components after the HTML tag they override.
    // Multi-letter all-caps (`HTTP`) has no lowercase letter and is still rejected.
    has_lower || stem.len() == 1
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(file_name) = ctx.path.file_name().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        let stem = file_name.split('.').next().unwrap_or(file_name);
        if super::is_sveltekit_route_file(file_name) {
            return Vec::new();
        }
        if super::is_file_based_route_segment(ctx.path, file_name) {
            return Vec::new();
        }
        if super::is_tanstack_vue_sfc_route(ctx.path, file_name) {
            return Vec::new();
        }
        // `404.vue` / `500.vue` are the idiomatic Vue Router HTTP-status error
        // pages; a purely numeric stem fits neither PascalCase nor kebab-case.
        // Unlike Next.js, the convention is not gated on a `pages/` directory.
        if super::is_numeric_stem(stem) {
            return Vec::new();
        }
        // A leading `_`/`__` marks a private/internal SFC (Storybook stories,
        // internal sub-components, fixtures); the convention is validated against
        // the name after the prefix, mirroring the TS text backend.
        let convention_stem = super::text::strip_private_prefix(stem);
        if stem.is_empty()
            || is_pascal_case(convention_stem)
            || super::text::is_kebab_case(convention_stem)
        {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "filename-naming-convention".into(),
            message: format!(
                "Vue SFC `{file_name}` should use PascalCase (e.g. `UserProfile.vue`) or kebab-case (e.g. `user-profile.vue`)."
            ),
            severity: Severity::Warning,
            span: None,
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), ""))
    }

    #[test]
    fn allows_pascal_case() {
        assert!(run("src/components/UserProfile.vue").is_empty());
    }

    #[test]
    fn allows_single_word_pascal() {
        assert!(run("src/App.vue").is_empty());
    }

    #[test]
    fn allows_multi_word_pascal() {
        assert!(run("src/components/Hook0CardHeader.vue").is_empty());
    }

    #[test]
    fn allows_sveltekit_page_component() {
        assert!(run("src/routes/users/+page.svelte").is_empty());
    }

    #[test]
    fn allows_kebab_case() {
        assert!(run("src/components/user-profile.vue").is_empty());
    }

    // Regression for #1424: kebab-case Vue SFC filenames are endorsed by the
    // official Vue style guide and must not be flagged.
    #[test]
    fn allows_kebab_case_panel_issue_1424() {
        assert!(run("app/src/panels/time-series/panel-time-series.vue").is_empty());
    }

    #[test]
    fn allows_kebab_case_app_root_issue_1424() {
        assert!(run("app/src/app.vue").is_empty());
    }

    // Regression for #1820: `index.vue` is the Vue Router / Nuxt file-system
    // routing convention for a directory's entry component — the same
    // index-as-entry convention as `index.ts`. Its `index` stem is lowercase,
    // so the kebab-case allowance must keep it from being flagged.
    #[test]
    fn allows_index_route_file_issue_1820() {
        assert!(run("demos/preview/index.vue").is_empty());
        assert!(run("demos/src/Marks/Underline/Vue/index.vue").is_empty());
    }

    // Regression for #1585: Nuxt file-based-routing dynamic-segment Vue SFCs
    // under a `pages/` directory are framework-mandated route filenames and must
    // not be flagged. The kebab/lowercase route files (`index.vue`, `about.vue`,
    // `default.vue`, `app.vue`) are already covered by the kebab-case allowance;
    // only the bracket-wrapped segments need the dedicated exemption.
    #[test]
    fn allows_nuxt_dynamic_param_route_issue_1585() {
        assert!(run("test/fixtures/basic/pages/[id].vue").is_empty());
    }

    #[test]
    fn allows_nuxt_catch_all_route_issue_1585() {
        assert!(run("test/fixtures/basic/app/pages/[...slug].vue").is_empty());
    }

    #[test]
    fn allows_nuxt_optional_dynamic_param_route_issue_1585() {
        assert!(run("test/fixtures/basic/pages/[[id]].vue").is_empty());
    }

    // Already-passing Nuxt route files: confirm the kebab-case allowance keeps
    // them quiet (these never reached the dynamic-route exemption).
    #[test]
    fn allows_nuxt_index_about_default_app_issue_1585() {
        assert!(run("test/fixtures/basic/pages/index.vue").is_empty());
        assert!(run("test/fixtures/basic/pages/about.vue").is_empty());
        assert!(run("test/fixtures/basic/app/layouts/default.vue").is_empty());
        assert!(run("test/fixtures/basic/app.vue").is_empty());
    }

    // Guard: the bracket exemption only applies inside a `pages/` tree. A
    // bracket-named Vue SFC elsewhere is not a route segment and still fires.
    #[test]
    fn flags_bracket_stem_outside_pages_issue_1585() {
        assert_eq!(run("src/components/[id].vue").len(), 1);
    }

    // Regression for #2149: TanStack Vue Router SFC route files name route
    // components `{route-name}.component.vue` / `.errorComponent.vue` /
    // `.notFoundComponent.vue`. The route name is kebab-case, a `$param`, the
    // `__root`/`_layout` pathless marker, `index`, or dotted path segments —
    // none of which the framework lets adopt PascalCase. Under a `routes/`
    // ancestor these must not be flagged.
    #[test]
    fn allows_tanstack_vue_kebab_route_component_issue_2149() {
        let base = "examples/vue/basic-file-based-sfc/src/routes";
        assert!(run(&format!("{base}/editing-a.component.vue")).is_empty());
        assert!(run(&format!("{base}/editing-b.component.vue")).is_empty());
        assert!(run(&format!("{base}/index.component.vue")).is_empty());
    }

    #[test]
    fn allows_tanstack_vue_param_route_component_issue_2149() {
        let base = "examples/vue/basic-file-based-sfc/src/routes";
        assert!(run(&format!("{base}/posts.$postId.component.vue")).is_empty());
        assert!(run(&format!("{base}/$postId.component.vue")).is_empty());
    }

    #[test]
    fn allows_tanstack_vue_pathless_route_component_issue_2149() {
        let base = "examples/vue/basic-file-based-sfc/src/routes";
        assert!(run(&format!("{base}/__root.component.vue")).is_empty());
        assert!(run(&format!("{base}/__root.notFoundComponent.vue")).is_empty());
        assert!(run(&format!("{base}/__root.errorComponent.vue")).is_empty());
        assert!(run(&format!("{base}/_layout.component.vue")).is_empty());
    }

    // Negative space for #2149: the exemption is gated on the `routes/` ancestor
    // and on the documented component-role suffix. A `.component.vue` file
    // outside `routes/` with a non-conforming stem still fires, and a route file
    // whose suffix is not a documented role is validated normally.
    #[test]
    fn flags_component_suffix_outside_routes_issue_2149() {
        // `my_route` is snake_case (non-conforming even after private-prefix
        // stripping); outside `routes/` it gets no exemption and still flags.
        assert_eq!(run("src/components/my_route.component.vue").len(), 1);
    }

    #[test]
    fn flags_non_role_dotted_suffix_under_routes_issue_2149() {
        // `.handler` is not a TanStack component role, so the file is validated
        // by the normal convention; the snake_case stem still flags.
        assert_eq!(
            run("examples/vue/basic-file-based-sfc/src/routes/my_route.handler.vue").len(),
            1
        );
    }

    // Regression for #3376: unplugin-vue-router (vue-router file-based routing)
    // mandates a richer route-segment grammar than the plain bracket dynamic
    // segment. Under a `pages/` or `routes/` ancestor these filenames define the
    // route path and cannot adopt PascalCase/kebab-case without breaking it.
    #[test]
    fn allows_vue_router_route_group_issue_3376() {
        assert!(run("packages/playground-file-based/src/pages/(home).vue").is_empty());
    }

    #[test]
    fn allows_vue_router_repeatable_param_issue_3376() {
        assert!(run("packages/playground-file-based/src/pages/blog/[slug]+.vue").is_empty());
    }

    #[test]
    fn allows_vue_router_optional_repeatable_param_issue_3376() {
        assert!(
            run("packages/playground-file-based/src/pages/blog/[[slugOptional]]+.vue").is_empty()
        );
    }

    #[test]
    fn allows_vue_router_inline_mixed_params_issue_3376() {
        assert!(
            run("packages/playground-file-based/src/pages/users/sub-[first]-[second].vue")
                .is_empty()
        );
    }

    #[test]
    fn allows_vue_router_layout_file_issue_3376() {
        assert!(run("packages/playground-file-based/src/pages/with-layout/+layout.vue").is_empty());
    }

    #[test]
    fn allows_vue_router_typed_param_issue_3376() {
        assert!(
            run("packages/playground-file-based/src/pages/[month=month-valibot].vue").is_empty()
        );
    }

    // Plain dynamic segment under a `routes/` ancestor (unplugin-vue-router can be
    // configured to use `routes/`); the Nuxt-only `pages/` gate previously missed
    // these.
    #[test]
    fn allows_vue_router_param_under_routes_issue_3376() {
        let base = "packages/router/e2e/unplugin/fixtures/filenames/routes";
        assert!(run(&format!("{base}/articles/[id].vue")).is_empty());
        assert!(run(&format!("{base}/optional/[[doc]].vue")).is_empty());
    }

    // Guard: the route-segment grammar is gated on a `pages/`/`routes/` ancestor.
    // A route-shaped name elsewhere is not a route module and still fires.
    #[test]
    fn flags_route_group_outside_route_dir_issue_3376() {
        assert_eq!(run("src/components/(home).vue").len(), 1);
    }

    #[test]
    fn flags_layout_marker_outside_route_dir_issue_3376() {
        assert_eq!(run("src/components/+layout.vue").len(), 1);
    }

    // Guard: an ordinary mis-named SFC under a `pages/` ancestor (no route-segment
    // shape) still fires — the exemption widens shapes, not the case rule.
    #[test]
    fn flags_mis_cased_file_under_pages_issue_3376() {
        assert_eq!(run("src/pages/user_profile.vue").len(), 1);
    }

    #[test]
    fn flags_snake_case() {
        assert_eq!(run("src/components/user_profile.vue").len(), 1);
    }

    #[test]
    fn flags_camel_case() {
        assert_eq!(run("src/components/userProfile.vue").len(), 1);
    }

    // Regression for #3294: a single uppercase letter (`P`, `A`) is valid
    // PascalCase. Nuxt Content prose components are named after the HTML tag they
    // override, so single-letter SFC names must not be flagged.
    #[test]
    fn allows_single_letter_pascal_case_issue_3294() {
        assert!(run("src/runtime/components/prose/P.vue").is_empty());
        assert!(run("src/runtime/components/prose/A.vue").is_empty());
    }

    // Guard for #3294: multi-letter all-caps (`HTTP`) has no lowercase letter and
    // is NOT PascalCase — it must still fire. This fails if the single-letter
    // allowance were widened to any all-caps stem.
    #[test]
    fn flags_multi_letter_all_caps_issue_3294() {
        assert_eq!(run("src/components/HTTP.vue").len(), 1);
    }

    // Guard for #3294: the single-letter allowance is PascalCase-only — a single
    // LOWERCASE letter (`p`) is NOT PascalCase (it fails the uppercase-first
    // check). At the rule level `p.vue` stays clean via the kebab-case allowance
    // (like `app.vue`), but the PascalCase classifier must reject it.
    #[test]
    fn single_lowercase_letter_is_not_pascal_case_issue_3294() {
        assert!(!is_pascal_case("p"));
        assert!(is_pascal_case("P"));
        assert!(!is_pascal_case("HTTP"));
    }

    // Regression for #3217: `404.vue` / `500.vue` are the idiomatic Vue Router
    // HTTP-status error pages. A purely numeric stem satisfies neither
    // PascalCase nor kebab-case, so without the numeric-stem exemption it would
    // fall through to the case check and flag. The exemption is unconditional —
    // these are valid in `views/`, `pages/`, or anywhere.
    #[test]
    fn allows_numeric_error_page_in_views_issue_3217() {
        assert!(run("packages/playground/src/views/404.vue").is_empty());
    }

    #[test]
    fn allows_numeric_error_page_in_pages_issue_3217() {
        assert!(run("src/pages/500.vue").is_empty());
    }

    // Negative space for #3217: the exemption is narrow — only purely numeric
    // stems are spared. A camelCase SFC stem (neither numeric, PascalCase, nor
    // kebab-case) still fires.
    #[test]
    fn flags_camel_case_not_widened_by_numeric_exemption_issue_3217() {
        assert_eq!(run("src/components/myComponent.vue").len(), 1);
    }

    // Regression for #4522: a leading `_`/`__` marks a private/internal Vue SFC
    // (Storybook stories, internal sub-components, fixtures); the convention is
    // validated against the name after the prefix, mirroring the TS text backend.
    // `_NavigationMenu` strips to `NavigationMenu`, valid PascalCase.
    #[test]
    fn allows_underscore_private_pascal_case_issue_4522() {
        assert!(run("packages/core/src/NavigationMenu/story/_NavigationMenu.vue").is_empty());
        assert!(run("packages/core/src/LinkGroup/story/_LinkGroup.vue").is_empty());
        assert!(run("packages/core/src/Radio/story/_Radio.vue").is_empty());
    }

    #[test]
    fn allows_double_underscore_private_pascal_case_issue_4522() {
        assert!(run("src/components/__Internal.vue").is_empty());
    }

    #[test]
    fn allows_underscore_private_kebab_case_issue_4522() {
        assert!(run("src/components/_my-component.vue").is_empty());
    }

    // Load-bearing guard for #4522: stripping the prefix does NOT blanket-exempt —
    // the remainder must still be a valid name. `_navigationMenu` strips to
    // `navigationMenu`, which is neither PascalCase nor kebab-case, so it still
    // fires. This fails if the prefix were treated as an unconditional allowance.
    #[test]
    fn flags_underscore_private_camel_case_remainder_issue_4522() {
        assert_eq!(run("src/components/_navigationMenu.vue").len(), 1);
    }

    // Guard for #4522: an all-underscore stem strips to an empty remainder, which
    // is neither PascalCase nor kebab-case, so it still fires.
    #[test]
    fn flags_all_underscore_stem_issue_4522() {
        assert_eq!(run("src/components/__.vue").len(), 1);
    }
}
