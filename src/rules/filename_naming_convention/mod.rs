//! filename-naming-convention

mod rust;
mod text;
mod vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "filename-naming-convention",
    description: "Filename does not match the expected naming convention for its language.",
    remediation: "Use kebab-case for JS/TS filenames (e.g. `user-profile.ts`), PascalCase or kebab-case for Vue SFC filenames (e.g. `UserProfile.vue` or `user-profile.vue`), and snake_case for Rust filenames (e.g. `user_profile.rs`).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["code-quality"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

use crate::rules::path_utils::is_sveltekit_route_file;

/// Returns `true` for TanStack Router pathless layout routes living under any
/// `routes/` ancestor directory, in either spelling:
/// - directory-style: the file name starts with `_` (`_authed.tsx`);
/// - flat-route style: the first dot-segment of the file name ends with `_`
///   (`posts_.$postId.tsx`), the trailing-`_` marker for a layout route that
///   contributes no path segment.
/// See https://tanstack.com/router/latest/docs/framework/react/routing/file-based-routing#pathless-layout-routes.
fn is_tanstack_pathless_route(path: &std::path::Path, file_name: &str) -> bool {
    let stem = file_name.split('.').next().unwrap_or(file_name);
    if !file_name.starts_with('_') && !stem.ends_with('_') {
        return false;
    }
    path.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("routes"))
}

/// Returns `true` for Next.js Pages Router file-router names that the framework
/// mandates, living under any `pages/` ancestor directory:
/// - a bracket-wrapped dynamic segment (`[id].tsx`, `[...slug].tsx`,
///   `[[...slug]].tsx`), whose routing base starts with `[` and ends with `]`;
/// - a purely numeric error-page stem (`404.tsx`, `500.tsx`).
///
/// Both forms are dictated by Next.js file-based routing and cannot adopt
/// kebab/camel/Pascal case without breaking the route.
/// See https://nextjs.org/docs/pages/building-your-application/routing/dynamic-routes.
fn is_nextjs_pages_router_file(path: &std::path::Path, file_name: &str, stem: &str) -> bool {
    // Catch-all segments (`[...slug].tsx`) contain dots inside the brackets, so
    // the routing base is the text before the *file* extension, not the
    // dot-split stem. Strip a single trailing extension to recover it.
    let routing_base = file_name.rsplit_once('.').map_or(file_name, |(base, _)| base);
    let is_dynamic_segment = routing_base.starts_with('[') && routing_base.ends_with(']');
    let is_numeric_page = !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit());
    if !is_dynamic_segment && !is_numeric_page {
        return false;
    }
    path.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("pages"))
}

/// Returns `true` for a SolidStart file-router name that the framework mandates,
/// living under any `routes/` ancestor directory, recognised by the leading
/// shape of the filename:
/// - a splat / catch-all segment starting with `[...` (`[...404].tsx`,
///   `[...stories].tsx`);
/// - a route group starting with `(` whose name carries a matching `)`, with an
///   optional name after the close paren (`(home).tsx`, `(group2).tsx`,
///   `(ignored)route0.tsx`), and any trailing dotted segments
///   (`(basic).browser.test.tsx`).
///
/// Both forms are dictated by SolidStart's file router and cannot adopt
/// kebab/camel/Pascal case without breaking the route.
/// See https://docs.solidjs.com/solid-start/building-your-application/routing.
fn is_solidstart_route_file(path: &std::path::Path, file_name: &str) -> bool {
    let is_route_shape = file_name.starts_with("[...")
        || (file_name.starts_with('(') && file_name[1..].contains(')'));
    if !is_route_shape {
        return false;
    }
    path.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("routes"))
}

/// Returns `true` for a Nuxt file-based-routing dynamic-segment Vue SFC living
/// under any `pages/` ancestor directory: a bracket-wrapped routing base
/// (`[id].vue`, `[...slug].vue`, `[[id]].vue`), where the segment before the
/// `.vue` extension starts with `[` and ends with `]`.
///
/// Nuxt maps these filenames to dynamic and catch-all URL params, so they cannot
/// adopt kebab/Pascal case without breaking the route.
/// See https://nuxt.com/docs/guide/directory-structure/pages#dynamic-routes.
fn is_nuxt_dynamic_route_file(path: &std::path::Path, file_name: &str) -> bool {
    // Catch-all segments (`[...slug].vue`) contain dots inside the brackets, so
    // the routing base is the text before the *file* extension, not the
    // dot-split stem. Strip a single trailing extension to recover it.
    let routing_base = file_name.rsplit_once('.').map_or(file_name, |(base, _)| base);
    if !(routing_base.starts_with('[') && routing_base.ends_with(']')) {
        return false;
    }
    path.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("pages"))
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Text(Box::new(text::Check))),
            (Language::JavaScript, Backend::Text(Box::new(text::Check))),
            (Language::Tsx, Backend::Text(Box::new(text::Check))),
            (Language::Rust, Backend::Text(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(vue::Check))),
        ],
    }
}
