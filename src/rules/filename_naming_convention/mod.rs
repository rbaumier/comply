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

/// Returns `true` for TanStack Router pathless layout routes: a file whose
/// name starts with `_` living under any `routes/` ancestor directory.
/// See https://tanstack.com/router/latest/docs/framework/react/routing/file-based-routing#pathless-routes.
fn is_tanstack_pathless_route(path: &std::path::Path, file_name: &str) -> bool {
    if !file_name.starts_with('_') {
        return false;
    }
    path.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("routes"))
}

/// Returns `true` for TanStack Router dynamic/splat routes: a file whose
/// name starts with `$` living under any `routes/` ancestor directory.
/// `$.tsx` is the catch-all splat route and `$param.tsx` is a path
/// parameter — both filenames are dictated by the framework's file router.
/// See https://tanstack.com/router/latest/docs/framework/react/routing/file-based-routing.
fn is_tanstack_dynamic_route(path: &std::path::Path, file_name: &str) -> bool {
    if !file_name.starts_with('$') {
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
