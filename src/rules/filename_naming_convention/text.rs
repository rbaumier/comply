use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns `true` when `stem` matches kebab-case: a lowercase ASCII letter
/// followed by lowercase alphanumerics optionally separated by single dashes.
/// Equivalent to the pattern `^[a-z][a-z0-9]*(-[a-z0-9]+)*$`.
pub(super) fn is_kebab_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    let mut prev_dash = false;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'-' {
            if prev_dash || i == 0 || i == bytes.len() - 1 {
                return false;
            }
            prev_dash = true;
        } else if b.is_ascii_lowercase() || b.is_ascii_digit() {
            prev_dash = false;
        } else {
            return false;
        }
    }
    true
}

fn is_composable_name(stem: &str) -> bool {
    stem.starts_with("use") && stem.len() > 3 && stem.as_bytes()[3].is_ascii_uppercase()
}

/// Returns `true` when `stem` is exactly a BCP 47 / CLDR locale tag of the form
/// `<language>_<COUNTRY>`: 2-3 lowercase ASCII letters (ISO 639 language), a
/// single underscore, then 2-3 uppercase ASCII letters (ISO 3166 region). The
/// underscore separator is mandated by intl conventions, so locale files such
/// as `ar_EG.ts`, `zh_CN.ts`, or `en_US.ts` cannot adopt kebab-case.
fn is_locale_tag(stem: &str) -> bool {
    let Some((language, country)) = stem.split_once('_') else {
        return false;
    };
    let is_iso_segment = |segment: &str, want_upper: bool| {
        (2..=3).contains(&segment.len())
            && segment.bytes().all(|b| {
                if want_upper {
                    b.is_ascii_uppercase()
                } else {
                    b.is_ascii_lowercase()
                }
            })
    };
    is_iso_segment(language, false) && is_iso_segment(country, true)
}

fn is_pascal_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_uppercase() {
        return false;
    }
    bytes.iter().all(|&b| b.is_ascii_alphanumeric())
}

fn is_camel_case(stem: &str) -> bool {
    if stem.is_empty() {
        return false;
    }
    let bytes = stem.as_bytes();
    if !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes.iter().all(|&b| b.is_ascii_alphanumeric())
}

/// Strips a leading run of convention-prefix sigils (`_` and `$`) from `stem`,
/// returning the remaining name. A leading `_`/`__` is the JS/TS
/// "private/internal module" signal (e.g. `_utils`, `__database`) and a leading
/// `$` is the framework-internal / reactive-value convention (e.g. Prisma's
/// `$extends`, jQuery, RxJS, SvelteKit `$lib`), so the convention is validated
/// against the name after the prefix. Returns an empty string for an
/// all-sigil stem; the bare-`$` case is allowed earlier, and an all-underscore
/// stem then fails every convention check and is still flagged.
fn strip_private_prefix(stem: &str) -> &str {
    stem.trim_start_matches(['_', '$'])
}

/// Returns `true` when `path` is an Angular public-API barrel: a `.ts` file
/// whose stem is `public_api` or `public-api`. ng-packagr names this source
/// entry — the file that enumerates a library package's exported surface via
/// `export *` and is referenced as `entryFile` in `ng-package.json` — by this
/// convention, with the snake_case spelling being the Angular standard. Renaming
/// it would break the package's export contract, so the snake/kebab stem is the
/// intended name, not a convention violation. Mirrors the public-API barrel
/// allowance in `avoid-re-export-all`.
fn is_public_api_barrel_file(path: &std::path::Path) -> bool {
    let is_ts = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ts" | "tsx" | "mts" | "cts")
    );
    is_ts
        && matches!(
            path.file_stem().and_then(|s| s.to_str()),
            Some("public_api" | "public-api")
        )
}

fn is_ts_or_jsx_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.ends_with(".ts")
        || s.ends_with(".tsx")
        || s.ends_with(".mts")
        || s.ends_with(".cts")
        || s.ends_with(".js")
        || s.ends_with(".jsx")
        || s.ends_with(".mjs")
        || s.ends_with(".cjs")
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(file_name) = ctx.path.file_name().and_then(|s| s.to_str()) else {
            return Vec::new();
        };
        let stem = file_name.split('.').next().unwrap_or(file_name);
        if stem.is_empty() {
            return Vec::new();
        }
        if super::is_sveltekit_route_file(file_name) {
            return Vec::new();
        }
        if super::is_tanstack_pathless_route(ctx.path, file_name) {
            return Vec::new();
        }
        if super::is_file_based_route_segment(ctx.path, file_name) {
            return Vec::new();
        }
        if super::is_nextjs_numeric_error_page(ctx.path, stem) {
            return Vec::new();
        }
        if is_public_api_barrel_file(ctx.path) {
            return Vec::new();
        }
        // A lone `$` stem is the framework-internal entry-point convention
        // (e.g. Prisma's `$.ts`, TanStack Router's splat `$.tsx`); it has no
        // remainder to validate, so allow it outright.
        if stem == "$" {
            return Vec::new();
        }
        // A leading `_`/`__` marks a private/internal module and a leading `$`
        // marks a framework-internal/reactive value; validate the convention
        // against the name after the prefix.
        let convention_stem = strip_private_prefix(stem);
        if is_kebab_case(convention_stem) {
            return Vec::new();
        }
        if is_composable_name(convention_stem) {
            return Vec::new();
        }
        if is_locale_tag(convention_stem) {
            return Vec::new();
        }
        if is_ts_or_jsx_file(ctx.path)
            && (is_pascal_case(convention_stem) || is_camel_case(convention_stem))
        {
            return Vec::new();
        }
        vec![Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: 1,
            column: 1,
            rule_id: "filename-naming-convention".into(),
            message: format!("Filename `{file_name}` does not match naming convention."),
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
    fn allows_kebab_case() {
        assert!(run("src/user-profile.ts").is_empty());
    }

    #[test]
    fn allows_single_word_lowercase() {
        assert!(run("src/index.ts").is_empty());
    }

    #[test]
    fn allows_kebab_case_with_digits() {
        assert!(run("src/oauth2-provider.ts").is_empty());
    }

    #[test]
    fn allows_camel_case_ts() {
        assert!(run("src/userProfile.ts").is_empty());
    }

    #[test]
    fn allows_camel_case_js() {
        assert!(run("src/dynamicMiddleware.js").is_empty());
    }

    #[test]
    fn allows_pascal_case_tsx() {
        assert!(run("src/UserProfile.tsx").is_empty());
    }

    #[test]
    fn allows_pascal_case_ts() {
        assert!(run("src/UserProfile.ts").is_empty());
    }

    #[test]
    fn flags_snake_case() {
        assert_eq!(run("src/user_profile.ts").len(), 1);
    }

    #[test]
    fn allows_sveltekit_route_module() {
        assert!(run("src/routes/users/+page.server.ts").is_empty());
    }

    #[test]
    fn flags_trailing_dash() {
        assert_eq!(run("src/user-.ts").len(), 1);
    }

    #[test]
    fn flags_double_dash() {
        assert_eq!(run("src/user--profile.ts").len(), 1);
    }

    #[test]
    fn allows_composable_camel_case() {
        assert!(run("src/composables/useColorMode.ts").is_empty());
    }

    #[test]
    fn allows_composable_with_queries() {
        assert!(run("src/composables/useServiceTokenQueries.ts").is_empty());
    }

    #[test]
    fn allows_non_composable_camel_case() {
        assert!(run("src/externalLinks.ts").is_empty());
    }

    #[test]
    fn allows_tanstack_pathless_layout_route_tsx() {
        assert!(run("src/app/routes/_authed.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_pathless_layout_route_test_tsx() {
        assert!(run("src/app/routes/_authed.test.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_pathless_layout_route_nested() {
        assert!(run("src/app/routes/dashboard/_layout.tsx").is_empty());
    }

    // Regression for #2147: TanStack Router flat-route files whose first
    // dot-segment ends with `_` are pathless layout routes (the flat-file
    // equivalent of `_prefix.tsx`); they must not be flagged inside `routes/`.
    #[test]
    fn allows_tanstack_trailing_underscore_layout_route_issue_2147() {
        assert!(run("src/routes/posts_.$postId.edit.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_trailing_underscore_simple_route_issue_2147() {
        assert!(run("src/routes/baz_.bar.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_trailing_underscore_nested_route_issue_2147() {
        assert!(run("src/routes/blog_.$blogId.$slug.route.tsx").is_empty());
    }

    // Guard: a stem with an underscore in the MIDDLE (not trailing) is not a
    // pathless layout route and still fires inside `routes/`.
    #[test]
    fn flags_underscore_middle_stem_inside_routes_issue_2147() {
        assert_eq!(run("src/routes/some_invalid_name.tsx").len(), 1);
    }

    // Guard: the trailing-underscore exemption is gated on `routes/`; the same
    // stem shape outside any `routes/` dir still fires.
    #[test]
    fn flags_trailing_underscore_stem_outside_routes_issue_2147() {
        assert_eq!(run("src/utils/foo_.ts").len(), 1);
    }

    #[test]
    fn allows_underscore_prefix_valid_remainder_outside_routes() {
        // `_authed` strips to `authed`, a valid camelCase name; the leading
        // underscore is the private-module signal, so it is allowed anywhere.
        assert!(run("src/app/_authed.tsx").is_empty());
    }

    // Regression for #521: TanStack Router splat/dynamic route files use `$`.
    #[test]
    fn allows_tanstack_splat_route_tsx_issue_521() {
        assert!(run("src/app/routes/$.tsx").is_empty());
    }

    #[test]
    fn allows_tanstack_splat_route_ts_issue_521() {
        assert!(run("src/app/routes/api/$.ts").is_empty());
    }

    #[test]
    fn allows_tanstack_dynamic_param_route() {
        assert!(run("src/app/routes/posts/$postId.tsx").is_empty());
    }

    // Regression for #1618: a leading `$` is the framework-internal / reactive
    // value convention (Prisma `$extends`, jQuery, RxJS, SvelteKit `$lib`);
    // `$`-prefixed JS/TS files are allowed anywhere, not only under `routes/`.
    #[test]
    fn allows_dollar_stem_anywhere_issue_1618() {
        assert!(run("packages/cli/src/platform/$.ts").is_empty());
    }

    #[test]
    fn allows_dollar_prefix_camel_remainder_issue_1618() {
        assert!(run("packages/client/src/runtime/core/extensions/$extends.ts").is_empty());
    }

    #[test]
    fn allows_dollar_prefix_pascal_remainder_issue_1618() {
        // `$BadName` strips to `BadName`, valid PascalCase for a .ts file.
        assert!(run("src/$BadName.ts").is_empty());
    }

    // Guard: stripping the `$` does not exempt an invalid remainder — a
    // snake_cased remainder after the sigil still fires.
    #[test]
    fn flags_dollar_prefix_snake_case_remainder_issue_1618() {
        assert_eq!(run("src/$bad_name.ts").len(), 1);
    }

    // Regression for #1399: TypeScript 4.7+ ESM/CJS extensions (.mts/.cts)
    // get the same camelCase/PascalCase allowance as .ts/.tsx.
    #[test]
    fn allows_camel_case_mts_issue_1399() {
        assert!(run("src/userProfile.mts").is_empty());
    }

    #[test]
    fn allows_pascal_case_cts_issue_1399() {
        assert!(run("src/UserProfile.cts").is_empty());
    }

    #[test]
    fn allows_kebab_case_mts_issue_1399() {
        assert!(run("src/user-profile.mts").is_empty());
    }

    #[test]
    fn allows_pascal_case_declaration_mts_issue_1399() {
        assert!(run("src/UserProfile.d.mts").is_empty());
    }

    // True positive: a genuinely snake_cased .mts/.cts filename still fires.
    #[test]
    fn flags_snake_case_mts_issue_1399() {
        assert_eq!(run("src/user_profile.mts").len(), 1);
    }

    #[test]
    fn flags_snake_case_cts_issue_1399() {
        assert_eq!(run("src/user_profile.cts").len(), 1);
    }

    // Regression for #1994: BCP 47 / CLDR locale filenames `<lang>_<COUNTRY>`
    // require the underscore separator and must not be flagged.
    #[test]
    fn allows_locale_tag_ar_eg_issue_1994() {
        assert!(run("src/locales/ar_EG.ts").is_empty());
    }

    #[test]
    fn allows_locale_tag_zh_cn_issue_1994() {
        assert!(run("src/locales/zh_CN.ts").is_empty());
    }

    #[test]
    fn allows_locale_tag_en_us_issue_1994() {
        assert!(run("src/locales/en_US.ts").is_empty());
    }

    #[test]
    fn allows_three_letter_language_locale_tag_issue_1994() {
        assert!(run("src/locales/fil_PH.ts").is_empty());
    }

    // Guard: ordinary snake_case files are not locale tags and still fire.
    #[test]
    fn flags_snake_case_non_locale_issue_1994() {
        assert_eq!(run("src/my_helper.ts").len(), 1);
    }

    // Guard: wrong case patterns are not exempted by the locale rule.
    #[test]
    fn flags_camel_concat_not_locale_issue_1994() {
        // `arEG` has no underscore separator; camelCase allowance handles it,
        // so the behavior is unchanged (no diagnostic) — but NOT via locale.
        assert!(!is_locale_tag("arEG"));
    }

    #[test]
    fn flags_screaming_snake_not_locale_issue_1994() {
        assert_eq!(run("src/API_KEYS.ts").len(), 1);
        assert!(!is_locale_tag("API_KEYS"));
    }

    // Regression for #1758: Next.js Pages Router dynamic-segment and numeric
    // error-page filenames are framework-mandated and must not be flagged.
    #[test]
    fn allows_nextjs_pages_dynamic_segment_issue_1758() {
        assert!(run("apps/nextjs-pages/src/pages/app/discussions/[discussionId].tsx").is_empty());
    }

    #[test]
    fn allows_nextjs_pages_numeric_error_page_issue_1758() {
        assert!(run("apps/nextjs-pages/src/pages/404.tsx").is_empty());
    }

    #[test]
    fn allows_nextjs_pages_nested_dynamic_segment_issue_1758() {
        assert!(run("apps/nextjs-pages/src/pages/public/discussions/[discussionId].tsx").is_empty());
    }

    #[test]
    fn allows_nextjs_pages_catch_all_segment_issue_1758() {
        assert!(run("src/pages/posts/[...slug].tsx").is_empty());
    }

    #[test]
    fn allows_nextjs_pages_optional_catch_all_segment_issue_1758() {
        assert!(run("src/pages/posts/[[...slug]].tsx").is_empty());
    }

    #[test]
    fn allows_nextjs_pages_500_error_page_issue_1758() {
        assert!(run("src/pages/500.tsx").is_empty());
    }

    // Guard: the bracket/numeric exemption only applies inside a `pages/` tree.
    #[test]
    fn flags_numeric_stem_outside_pages_issue_1758() {
        assert_eq!(run("src/app/404.tsx").len(), 1);
    }

    #[test]
    fn flags_bracket_stem_outside_pages_issue_1758() {
        assert_eq!(run("src/app/[discussionId].tsx").len(), 1);
    }

    #[test]
    fn flags_wrong_case_locale_shape_issue_1994() {
        // `Ar_eg` inverts the required case pattern, so it is not a locale tag.
        assert!(!is_locale_tag("Ar_eg"));
        assert_eq!(run("src/Ar_eg.ts").len(), 1);
    }

    // Regression for #1616: a leading `_`/`__` is the JS/TS private-module
    // signal; the convention is validated against the name after the prefix.
    #[test]
    fn allows_single_underscore_private_camel_case_issue_1616() {
        assert!(run("packages/generator-helper/src/_utils.ts").is_empty());
    }

    #[test]
    fn allows_double_underscore_private_camel_case_issue_1616() {
        assert!(run("src/__tests__/integration/postgresql/__database.ts").is_empty());
    }

    #[test]
    fn allows_underscore_private_pascal_case_issue_1616() {
        assert!(run("src/_Platform.ts").is_empty());
    }

    #[test]
    fn allows_underscore_private_kebab_case_issue_1616() {
        assert!(run("src/_user-profile.ts").is_empty());
    }

    // Guard: stripping the prefix does not exempt an invalid remainder.
    #[test]
    fn flags_underscore_private_snake_case_remainder_issue_1616() {
        assert_eq!(run("src/_user_profile.ts").len(), 1);
    }

    // Guard: an all-underscore stem has an empty remainder and still fires.
    #[test]
    fn flags_all_underscore_stem_issue_1616() {
        assert_eq!(run("src/__.ts").len(), 1);
    }

    // Regression for #1534: Angular's ng-packagr `public_api.ts` library barrel
    // is a framework-mandated entry filename and must not be flagged.
    #[test]
    fn allows_angular_public_api_snake_case_barrel_issue_1534() {
        assert!(run("packages/misc/angular-in-memory-web-api/public_api.ts").is_empty());
    }

    #[test]
    fn allows_angular_public_api_nested_barrel_issue_1534() {
        assert!(run("schematics-for-libraries/projects/my-lib/src/public_api.ts").is_empty());
    }

    #[test]
    fn allows_angular_public_api_kebab_case_barrel_issue_1534() {
        assert!(run("src/cdk/tree/public-api.ts").is_empty());
    }

    // Guard: a snake_case file that merely contains `public_api` as a substring
    // of a longer name is an ordinary module, not the barrel, and must still be
    // flagged — the exemption matches the exact stem only, not a substring.
    #[test]
    fn flags_public_api_substring_file_issue_1534() {
        assert_eq!(run("src/public_api_helper.ts").len(), 1);
        assert_eq!(run("src/public_api_registry.ts").len(), 1);
    }

    // Guard: the exemption does not loosen the case rule — a genuinely
    // snake_cased file still fires, only the exact barrel stem is exempt.
    #[test]
    fn flags_snake_case_still_fires_after_public_api_exemption_issue_1534() {
        assert_eq!(run("src/user_profile.ts").len(), 1);
    }

    // Regression for #2223: SolidStart file-router conventions under a `routes/`
    // ancestor — splat/catch-all (`[...name]`), bare route groups (`(group)`),
    // and prefixed group routes (`(group)name`) — are framework-mandated and
    // must not be flagged.
    #[test]
    fn allows_solidstart_splat_route_issue_2223() {
        assert!(run("apps/tests/src/routes/[...404].tsx").is_empty());
    }

    #[test]
    fn allows_solidstart_splat_named_route_issue_2223() {
        assert!(run("apps/fixtures/hackernews/src/routes/[...stories].tsx").is_empty());
    }

    #[test]
    fn allows_solidstart_route_group_issue_2223() {
        assert!(run("apps/tests/src/routes/(home).tsx").is_empty());
    }

    #[test]
    fn allows_solidstart_route_group_with_digits_issue_2223() {
        assert!(run("apps/fixtures/experiments/src/routes/(group2).tsx").is_empty());
    }

    #[test]
    fn allows_solidstart_prefixed_group_route_issue_2223() {
        assert!(
            run("apps/fixtures/experiments/src/routes/nested/(level1)/(ignored)route0.tsx")
                .is_empty()
        );
    }

    #[test]
    fn allows_solidstart_route_group_with_dotted_segments_issue_2223() {
        assert!(run("apps/tests/src/routes/(basic).browser.test.tsx").is_empty());
    }

    #[test]
    fn allows_solidstart_nested_route_group_issue_2223() {
        assert!(run("apps/fixtures/experiments/src/routes/test/(hi).tsx").is_empty());
    }

    // Guard: the exemption is `routes/`-scoped — the same shapes outside a
    // `routes/` ancestor are NOT framework routes and still fire.
    #[test]
    fn flags_solidstart_splat_shape_outside_routes_issue_2223() {
        assert_eq!(run("src/app/[...404].tsx").len(), 1);
    }

    #[test]
    fn flags_solidstart_group_shape_outside_routes_issue_2223() {
        assert_eq!(run("src/app/(home).tsx").len(), 1);
    }

    // Guard: an ordinary mis-cased file under `routes/` does not match the
    // SolidStart shapes and still fires.
    #[test]
    fn flags_mis_cased_file_under_routes_issue_2223() {
        assert_eq!(run("src/routes/my_component.tsx").len(), 1);
    }
}
