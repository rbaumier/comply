//! vue-url-state-for-filters text backend.
//!
//! Flags `ref(...)` / `reactive(...)` declarations whose name strongly
//! suggests filter or pagination state (`page`, `pageSize`, `filters`,
//! `search`, `query`, `sort`, `sortBy`, `limit`, `offset`). That kind of
//! state should survive reloads and be shareable, so it belongs in the
//! URL (`useUrlSearchParams`, router query).
//!
//! Only a route component is checked (see [`is_route_component`]). The router
//! instantiates one such component per URL, so that component owns the query
//! string. A component the page embeds — a widget, a form field, a dialog, a
//! story — may be mounted several times over a single URL, where filter state
//! cannot move to the query string without collisions. A story colocated with
//! the view it demonstrates (`src/views/UserList.story.vue`) sits under a route
//! root anyway, and is skipped through `ctx.file.path_segments.in_storybook`.
//!
//! The detector suppresses itself when the file already references
//! `useUrlSearchParams`, `useRouteQuery`, or assigns to `route.query` —
//! those are the blessed patterns.
//!
//! Individual candidates are also exempted when their actual usage shows
//! they are not page-level filter state:
//! - validation-constraint parameter: the var feeds a schema-validation
//!   constraint (`.max(<name>)` / `.min(<name>)` / `.length(<name>)`) and the
//!   file uses a validation library (`yup`, `zod`, `valibot`, `joi`,
//!   `superstruct`, `arktype`);
//! - widget named v-model binding: the var is the bound value of a
//!   `v-model:<arg>="<name>"` directive (widget-scoped two-way state).

use std::path::Path;

use crate::diagnostic::{Diagnostic, Severity};
use crate::project::ProjectCtx;
use crate::rules::backend::{CheckCtx, TextCheck};

/// Directory roots a Vue router mounts its components from: `pages/` (the
/// file-based routing of Nuxt and unplugin-vue-router), `views/` (the Vue CLI
/// and Vite templates), and `routes/` (the layout Directus-style apps use).
const ROUTE_ROOT_SEGMENTS: &[&str] = &["pages", "views", "routes"];

/// Identifiers that strongly indicate filter/pagination state.
const FILTER_NAMES: &[&str] = &[
    "page",
    "pageSize",
    "pageIndex",
    "currentPage",
    "perPage",
    "pagination",
    "filter",
    "filters",
    "activeFilters",
    "selectedFilters",
    "search",
    "searchQuery",
    "searchTerm",
    "query",
    "sort",
    "sortBy",
    "sortOrder",
    "sortField",
    "sortDirection",
    "orderBy",
    "offset",
    "limit",
    "cursor",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("ref(") && !src.contains("reactive(") {
            return Vec::new();
        }
        if ctx.file.path_segments.in_storybook || !is_route_component(ctx.path, ctx.project) {
            return Vec::new();
        }
        // If the file already uses URL-backed state, trust the author.
        if src.contains("useUrlSearchParams")
            || src.contains("useRouteQuery")
            || src.contains("route.query")
            || src.contains("$route.query")
        {
            return Vec::new();
        }

        let mut diags = Vec::new();
        for (i, line) in src.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }

            let Some(name) = declared_ref_name(trimmed) else {
                continue;
            };
            if !is_filter_name(name) {
                continue;
            }
            if used_as_validation_constraint(src, name) || bound_as_named_vmodel(src, name) {
                continue;
            }

            diags.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: i + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{name}` looks like filter/pagination state — store it in the URL \
                     (`useUrlSearchParams` or router query) so it survives reloads and is shareable."
                ),
                severity: Severity::Error,
                span: None,
            });
        }
        diags
    }
}

/// True when `path` follows the layout of a component a router mounts for a
/// URL: under one of [`ROUTE_ROOT_SEGMENTS`], and outside any `components/`
/// directory — where Vue apps keep the components a page embeds.
///
/// Only the segments below the app's own root count. That root is the directory
/// owning the nearest `package.json`, so a monorepo package reads its own
/// layout and not the names of its siblings; the scan root answers when no
/// manifest is in reach. Above it, the directories name the machine the scan
/// runs on.
///
/// This reads the layout, not the route table, so an app that registers its
/// components in a hand-written `createRouter({ routes })` from arbitrary
/// directories is out of the rule's reach. Under-claiming that way is
/// deliberate: the query string belongs to whichever component the router
/// resolves the URL to, and the layout is the only evidence available here.
fn is_route_component(path: &Path, project: &ProjectCtx) -> bool {
    let app_root = project
        .nearest_package_json_dir(path)
        .or_else(|| project.project_root.clone());
    let relative = app_root
        .as_deref()
        .and_then(|root| path.strip_prefix(root).ok())
        .unwrap_or(path);
    let mut is_under_route_root = false;
    for segment in relative.components() {
        let Some(name) = segment.as_os_str().to_str() else {
            continue;
        };
        if name.eq_ignore_ascii_case("components") {
            return false;
        }
        is_under_route_root |=
            ROUTE_ROOT_SEGMENTS.iter().any(|root| name.eq_ignore_ascii_case(root));
    }
    is_under_route_root
}

/// Return the declared identifier from a line of the form
/// `const <name> = ref(...)` / `reactive(...)`. Returns `None` otherwise.
fn declared_ref_name(line: &str) -> Option<&str> {
    let rest = if let Some(r) = line.strip_prefix("const ") {
        r
    } else if let Some(r) = line.strip_prefix("let ") {
        r
    } else {
        return None;
    };
    let rest = rest.trim_start();
    // Destructuring is not supported.
    if rest.starts_with('{') || rest.starts_with('[') {
        return None;
    }
    let name_end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))?;
    let name = &rest[..name_end];
    if name.is_empty() {
        return None;
    }
    let after = rest[name_end..].trim_start();
    // Optional type annotation: `const foo: T = ref(...)`.
    let after = if let Some(a) = after.strip_prefix(':') {
        // Skip until `=`.
        let eq = a.find('=')?;
        a[eq..].trim_start()
    } else {
        after
    };
    let after = after.strip_prefix('=')?.trim_start();
    if after.starts_with("ref(")
        || after.starts_with("reactive(")
        || after.starts_with("shallowRef(")
    {
        Some(name)
    } else {
        None
    }
}

fn is_filter_name(name: &str) -> bool {
    FILTER_NAMES.contains(&name)
}

/// Validation libraries whose constraint methods exempt a filter-named var.
const VALIDATION_LIBS: &[&str] = &["yup", "zod", "valibot", "joi", "superstruct", "arktype"];

/// True when `name` flows into a schema-validation constraint method
/// (`.max(<name>`, `.min(<name>`, `.length(<name>`) and the file uses a
/// validation library. The library gate prevents exempting a pagination
/// `limit` passed to `Math.max(limit, 10)`.
fn used_as_validation_constraint(src: &str, name: &str) -> bool {
    if !VALIDATION_LIBS.iter().any(|lib| src.contains(lib)) {
        return false;
    }
    for method in [".max(", ".min(", ".length("] {
        let needle = format!("{method}{name}");
        let mut from = 0;
        while let Some(rel) = src[from..].find(&needle) {
            let after = from + rel + needle.len();
            let boundary = src[after..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '$');
            if boundary {
                return true;
            }
            from = after;
        }
    }
    false
}

/// True when `name` is the bound value of a `v-model:<arg>="<name>"`
/// directive — widget-scoped two-way state, not page-level filter state.
fn bound_as_named_vmodel(src: &str, name: &str) -> bool {
    for (idx, _) in src.match_indices("v-model:") {
        let rest = &src[idx..];
        let Some(eq) = rest.find('=') else { continue };
        let after = rest[eq + 1..].trim_start();
        for q in ['"', '\''] {
            if let Some(s) = after.strip_prefix(q)
                && let Some(end) = s.find(q)
                && s[..end].trim() == name
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        run_at("src/pages/List.vue", src)
    }

    fn run_at(path: &str, src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), src))
    }

    #[test]
    fn flags_page_ref() {
        assert_eq!(run("const page = ref(1)").len(), 1);
    }

    #[test]
    fn flags_filters_reactive() {
        assert_eq!(run("const filters = reactive({ status: 'open' })").len(), 1);
    }

    #[test]
    fn flags_typed_search_ref() {
        assert_eq!(run("const search: string = ref('')").len(), 1);
    }

    #[test]
    fn flags_sort_by() {
        assert_eq!(run("const sortBy = ref('name')").len(), 1);
    }

    #[test]
    fn allows_url_search_params_in_file() {
        let src = "const params = useUrlSearchParams('history')\nconst page = ref(1)";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_route_query_in_file() {
        let src = "const q = route.query\nconst filters = reactive({})";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_non_filter_name() {
        assert!(run("const count = ref(0)").is_empty());
    }

    #[test]
    fn allows_non_ref_binding() {
        assert!(run("const page = computed(() => 1)").is_empty());
    }

    #[test]
    fn ignores_comment_lines() {
        assert!(run("// const page = ref(1)").is_empty());
    }

    #[test]
    fn allows_limit_as_validation_constraint() {
        let src = "const limit = ref(5)\nconst schema = yup.object({ content: yup.string().max(limit.value) })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_search_term_as_named_vmodel() {
        let src = "const searchTerm = ref('')\n<UListbox v-model:search-term=\"searchTerm\" :items=\"items\" />";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_limit_without_validation_lib() {
        let src = "const limit = ref(5)\nconst capped = Math.max(limit.value, 10)";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_search_term_without_vmodel() {
        assert_eq!(run("const searchTerm = ref('')").len(), 1);
    }

    /// The two files reported in issue #6850, from unovue/radix-vue: Histoire
    /// stories live outside any route root, so the component they render owns
    /// no query string.
    #[test]
    fn allows_filter_refs_in_histoire_story_files() {
        assert!(
            run_at("packages/core/src/Combobox/story/_Combobox.vue", "const query = ref('')")
                .is_empty()
        );
        assert!(
            run_at(
                "packages/core/src/Listbox/story/ListboxFilter.story.vue",
                "const searchTerm = ref('')"
            )
            .is_empty()
        );
    }

    /// A story colocated with the view it demonstrates sits under a route root,
    /// so the route gate alone lets it through and the story-catalog lever is
    /// what stops it.
    #[test]
    fn allows_filter_refs_in_story_colocated_under_a_route_root() {
        let path = Path::new("src/views/UserList.story.vue");
        assert!(is_route_component(path, crate::project::default_static_project_ctx()));
        let file = FileCtx {
            path_segments: PathSegments { in_storybook: true, ..PathSegments::default() },
            ..FileCtx::default()
        };
        assert!(
            Check.check(&CheckCtx::for_test_with_file(path, "const page = ref(1)", &file))
                .is_empty()
        );
    }

    /// The directories above the project root name the machine the scan runs
    /// on: a checkout under `~/dev/components/` must not turn the rule off.
    #[test]
    fn reads_no_path_segment_above_the_project_root() {
        let dir = tempfile::TempDir::new().unwrap();
        let rooted_at = |root: &Path| {
            let mut project = ProjectCtx::empty();
            project.project_root = Some(root.to_path_buf());
            project
        };

        let root = dir.path().join("components/app");
        assert!(is_route_component(&root.join("src/pages/List.vue"), &rooted_at(&root)));

        let root = dir.path().join("views/app");
        assert!(!is_route_component(&root.join("src/widgets/DataTable.vue"), &rooted_at(&root)));
    }

    /// The app's root is the directory owning its `package.json`: the editor
    /// lints one buffer with no scan root, and a monorepo scan has a root that
    /// sits above the package whose layout is being read.
    #[test]
    fn reads_the_layout_from_the_package_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let app = dir.path().join("components/app");
        std::fs::create_dir_all(app.join("src/pages")).unwrap();
        std::fs::write(app.join("package.json"), r#"{"name":"app","version":"1.0.0"}"#).unwrap();

        assert!(is_route_component(&app.join("src/pages/List.vue"), &ProjectCtx::empty()));

        let mut monorepo = ProjectCtx::empty();
        monorepo.project_root = Some(dir.path().to_path_buf());
        assert!(is_route_component(&app.join("src/pages/List.vue"), &monorepo));
    }

    /// A widget the page embeds may be mounted several times over one URL, so
    /// its filter state cannot move to the query string.
    #[test]
    fn allows_filter_ref_outside_a_route_root() {
        assert!(run_at("src/components/DataTable.vue", "const page = ref(1)").is_empty());
        assert!(
            run_at("app/src/interfaces/list-m2m/list-m2m.vue", "const limit = ref(5)").is_empty()
        );
        // A `components/` directory nested under a route root still holds
        // embedded components, not the one the route resolves to.
        assert!(
            run_at("src/views/private/components/notifications-drawer.vue", "const page = ref(1)")
                .is_empty()
        );
    }

    #[test]
    fn flags_filter_ref_under_every_route_root() {
        assert_eq!(run_at("pages/users/index.vue", "const page = ref(1)").len(), 1);
        assert_eq!(run_at("src/views/UserList.vue", "const page = ref(1)").len(), 1);
        assert_eq!(run_at("app/src/modules/x/routes/runs.vue", "const page = ref(1)").len(), 1);
    }

    /// A directory name is a convention, not an identifier: `Views/` and
    /// `Components/` carry the same meaning as their lowercase spelling.
    #[test]
    fn reads_directory_names_case_insensitively() {
        assert_eq!(run_at("src/Views/UserList.vue", "const page = ref(1)").len(), 1);
        assert!(run_at("src/Views/Components/Table.vue", "const page = ref(1)").is_empty());
    }
}
