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
//!
//! Individual candidates are also exempted when their actual usage shows
//! they are not page-level filter state:
//! - validation-constraint parameter: the var feeds a schema-validation
//!   constraint (`.max(<name>)` / `.min(<name>)` / `.length(<name>)`) and the
//!   file uses a validation library (`yup`, `zod`, `valibot`, `joi`,
//!   `superstruct`, `arktype`);
//! - widget named v-model binding: the var is the bound value of a
//!   `v-model:<arg>="<name>"` directive (widget-scoped two-way state).

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
}
