//! next-no-font-link — `<link href="...fonts.googleapis.com..." />` should be
//! replaced by `next/font` for self-hosting and zero CLS.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "next-no-font-link",
    description: "Loading Google Fonts via `<link>` — use `next/font` for self-hosting and zero layout shift.",
    remediation: "Replace `<link href=\"https://fonts.googleapis.com/...\" />` with `next/font`, e.g. \
                  `import { Inter } from 'next/font/google'; const inter = Inter({ subsets: ['latin'] });`. \
                  `next/font` self-hosts the font, eliminates a render-blocking request, and \
                  reserves layout space to avoid CLS.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nextjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
