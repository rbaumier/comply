//! vue-url-state-for-filters text backend.
//!
//! Flags `ref(...)` / `reactive(...)` declarations whose name strongly
//! suggests filter or pagination state (`page`, `pageSize`, `filters`,
//! `search`, `query`, `sort`, `sortBy`, `limit`, `offset`). That kind of
//! state should survive reloads and be shareable, so it belongs in the
//! URL (`useUrlSearchParams`, router query).
//!
//! The detector suppresses itself when the file already references
//! `useUrlSearchParams`, `useRouteQuery`, or assigns to `route.query` —
//! those are the blessed patterns.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

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

            diags.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: i + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{name}` looks like filter/pagination state — store it in the URL \
                     (`useUrlSearchParams` or router query) so it survives reloads and is shareable."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        diags
    }
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
    let name_end = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$'))?;
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
    if after.starts_with("ref(") || after.starts_with("reactive(") || after.starts_with("shallowRef(")
    {
        Some(name)
    } else {
        None
    }
}

fn is_filter_name(name: &str) -> bool {
    FILTER_NAMES.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("List.vue"), src))
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
}
