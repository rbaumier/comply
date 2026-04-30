//! no-full-import

mod typescript;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
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
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
