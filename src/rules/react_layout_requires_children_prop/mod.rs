//! A Next.js App Router `layout.tsx` receives `children: ReactNode` from the
//! router. A layout whose default export silently drops `children` renders
//! an empty page — flag layouts that don't destructure or reference it.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-layout-requires-children-prop",
    description: "App Router layouts must accept and render `children`.",
    remediation: "Destructure `children` from props and render it inside the \
                  layout's markup: `export default function Layout({ children }) {}`.",
    severity: Severity::Error,
    doc_url: Some("https://nextjs.org/docs/app/api-reference/file-conventions/layout"),
    categories: &["react", "nextjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
