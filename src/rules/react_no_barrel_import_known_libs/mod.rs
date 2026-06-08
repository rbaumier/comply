//! react-no-barrel-import-known-libs — barrel (root) imports from icon/UI/util packages.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-barrel-import-known-libs",
    description: "Named imports from known non-tree-shakeable barrel packages \
                  (@mui/material, @mui/icons-material, lodash, date-fns) pull the \
                  whole library into the bundle.",
    remediation: "Import from the library's subpath (e.g. `lodash/debounce`, \
                  `@mui/material/Button`) so bundlers can tree-shake effectively. \
                  Tree-shakeable icon/component libraries (lucide-react, \
                  @heroicons/react, @phosphor-icons/react, react-icons) are \
                  exempt.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "imports"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
