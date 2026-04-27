//! elysia-html-import-uppercase

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "elysia-html-import-uppercase",
    description: "Files using `@elysiajs/html` must import `Html` (uppercase) for the JSX factory.",
    remediation: "Import `{ Html }` from `@elysiajs/html` so JSX is transformed correctly: `import { Html } from '@elysiajs/html'`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["correctness", "elysia"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
