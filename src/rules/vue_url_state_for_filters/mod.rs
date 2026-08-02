//! vue-url-state-for-filters — filter/pagination state should live in the URL.

mod text;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "vue-url-state-for-filters",
    description: "Store filter/pagination state in the URL, not in local `ref()`.",
    remediation: "Filters, pagination, search, and sort state should survive a page \
                  reload and be shareable by URL. Use `useUrlSearchParams` from \
                  VueUse (or your router's query) instead of a local `ref()`.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: true,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Vue, Backend::Text(Box::new(text::Check)))],
    }
}

#[cfg(test)]
mod tests {
    use super::META;
    use crate::files::Language;
    use crate::rules::file_ctx::FileCtx;
    use std::path::Path;

    fn applies(path: &str) -> bool {
        let project = crate::project::default_static_project_ctx();
        let file = FileCtx::build(Path::new(path), "", Language::Vue, project);
        META.applies_to_file(&file)
    }

    /// A demo page under `docs/`/`examples/` keeps its pagination local on
    /// purpose — that is what it demonstrates — even when it sits under a
    /// route root.
    #[test]
    fn does_not_apply_in_a_relaxed_dir() {
        assert!(!applies("docs/examples/pages/UserList.vue"));
        assert!(applies("src/pages/UserList.vue"));
    }
}
