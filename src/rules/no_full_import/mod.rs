//! no-full-import

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-full-import",
    description: "Full-library default imports from `lodash`/`underscore`/`ramda` bloat bundles.",
    remediation: "Import individual functions: `import debounce from 'lodash/debounce'` or `import { debounce } from 'lodash-es'`.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://github.com/jfmengels/eslint-plugin-lodash-fp/blob/master/docs/rules/use-fp.md",
    ),
    categories: &["imports"],

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
