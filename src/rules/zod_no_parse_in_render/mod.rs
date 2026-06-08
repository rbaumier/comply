//! zod-no-parse-in-render — `.parse()` inside a React component body
//! re-runs every render and throws on bad data, blowing up the tree.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "zod-no-parse-in-render",
    description: "`schema.parse()` in a render path re-validates every render and throws on bad data.",
    remediation: "Validate at the data fetch boundary (`queryFn`, server action) or in `useMemo`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod", "react", "performance"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Tsx, Backend::Text(Box::new(typescript::Check)))],
    }
}
