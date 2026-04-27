//! next-no-sync-scripts

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-sync-scripts",
    description: "Synchronous `<script>` tags block parsing and hurt LCP.",
    remediation: "Use the `Script` component from `next/script` so Next.js can defer/optimise loading.",
    severity: Severity::Warning,
    doc_url: Some("https://nextjs.org/docs/messages/no-sync-scripts"),
    categories: &["nextjs", "performance"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
