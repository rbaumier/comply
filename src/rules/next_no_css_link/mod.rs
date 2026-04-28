//! next-no-css-link — `<link rel="stylesheet" />` bypasses Next.js bundling;
//! import CSS directly so it can be optimized and code-split.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-css-link",
    description: "`<link rel=\"stylesheet\" />` — import CSS directly for bundling and optimization.",
    remediation: "Replace `<link rel=\"stylesheet\" href=\"/foo.css\" />` with `import './foo.css'` \
                  in the relevant component or layout. Next.js then bundles, minifies, and \
                  code-splits the stylesheet alongside the JS chunk that needs it.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nextjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
