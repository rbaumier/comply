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

/// Returns `true` when any ancestor directory of `path` is named `routes`, the
/// gate shared by every TanStack / SolidStart file-router exemption: it scopes
/// the framework-mandated naming allowance to actual route modules, leaving a
/// like-named file elsewhere flaggable.
fn has_routes_ancestor(path: &std::path::Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == std::ffi::OsStr::new("routes"))
}

/// Returns `true` when any ancestor directory of `path` is named `pages` or
/// `routes` — the directory gate shared by file-based routers. unplugin-vue-router
/// defaults to `src/pages` but is configurable to `routes`, Nuxt uses `pages`,
/// Next.js Pages Router uses `pages`, and SolidStart / TanStack use `routes`, so
/// both names scope the route-segment naming allowance to actual route modules.
fn has_route_dir_ancestor(path: &std::path::Path) -> bool {
    path.components()
        .any(|c| matches!(c.as_os_str().to_str(), Some("pages" | "routes")))
}

/// Returns `true` for a file-based-routing route-segment filename, recognised by
/// the routing grammar shared across vue-router (unplugin-vue-router), Nuxt,
/// Next.js Pages Router, SolidStart, and TanStack Router. The route path is
/// derived from the filename, so the developer cannot rename it to
/// kebab/Pascal/camel case without breaking the route. The routing base is the
/// filename with a single trailing extension stripped (catch-all segments such
/// as `[...slug].vue` carry dots inside the brackets, so the dot-split stem is
/// not the routing base). A routing base is a route segment when it:
/// - contains a bracket-wrapped dynamic param anywhere — plain `[id]`, catch-all
///   `[...slug]`, optional `[[id]]`, repeatable `[slug]+` / `[[opt]]+`, typed
///   `[month=month-valibot]`, or inline-mixed `sub-[first]-[second]` — detected
///   by containing both `[` and `]`;
/// - is a route group `(name)` — starts with `(` and carries a matching `)`;
/// - is a layout / server marker starting with `+` (`+layout.vue`).
///
/// See https://uvr.esm.is/, https://nuxt.com/docs/guide/directory-structure/pages,
/// https://docs.solidjs.com/solid-start/building-your-application/routing.
fn is_file_based_route_segment(path: &std::path::Path, file_name: &str) -> bool {
    let routing_base = file_name.rsplit_once('.').map_or(file_name, |(base, _)| base);
    let is_route_shape = (routing_base.contains('[') && routing_base.contains(']'))
        || (routing_base.starts_with('(') && routing_base[1..].contains(')'))
        || routing_base.starts_with('+');
    if !is_route_shape {
        return false;
    }
    has_route_dir_ancestor(path)
}

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
    has_routes_ancestor(path)
}

/// Returns `true` for a TanStack Vue Router SFC route file living under any
/// `routes/` ancestor directory. The convention names route components
/// `{route-name}.component.vue`, `{route-name}.errorComponent.vue`, and
/// `{route-name}.notFoundComponent.vue`, where the dot-segment immediately
/// before `.vue` is the framework-fixed component role and everything before it
/// is the route name. The route name follows TanStack's route-path grammar
/// (kebab-case `editing-a`, `$param` segments, the `__root` / `_layout`
/// pathless markers, `index`, and dotted path segments such as `posts.$postId`),
/// none of which can adopt PascalCase without breaking the file-based router, so
/// the component-role suffix alone identifies the file and the route name is not
/// validated.
/// See https://tanstack.com/router/latest/docs/framework/vue/routing/file-based-routing.
fn is_tanstack_vue_sfc_route(path: &std::path::Path, file_name: &str) -> bool {
    let Some(stem) = file_name.strip_suffix(".vue") else {
        return false;
    };
    let Some((route_name, role)) = stem.rsplit_once('.') else {
        return false;
    };
    if !matches!(role, "component" | "errorComponent" | "notFoundComponent") {
        return false;
    }
    if route_name.is_empty() {
        return false;
    }
    has_routes_ancestor(path)
}

/// Returns `true` for a Next.js Pages Router numeric error-page stem (`404.tsx`,
/// `500.tsx`) living under any `pages/` ancestor directory. The stem is dictated
/// by Next.js file-based routing and cannot adopt kebab/camel/Pascal case without
/// breaking the error page. Bracket dynamic segments under `pages/` are handled
/// by the shared `is_file_based_route_segment`.
/// See https://nextjs.org/docs/pages/building-your-application/routing/custom-error.
fn is_nextjs_numeric_error_page(path: &std::path::Path, stem: &str) -> bool {
    let is_numeric_page = !stem.is_empty() && stem.bytes().all(|b| b.is_ascii_digit());
    if !is_numeric_page {
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
