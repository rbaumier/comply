//! eslint-plugin-next rules delegated to oxlint.

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY, oxlint_delegate};

pub fn register_all() -> Vec<RuleDef> {
    vec![
        oxlint_delegate(
            RuleMeta {
                id: "next-no-before-interactive-script-outside-document",
                description: "Don't use `next/script`'s `beforeInteractive` strategy outside \
                              `pages/_document.js`.",
                remediation: "Move the `<Script strategy=\"beforeInteractive\">` into \
                              `pages/_document.js`. The `beforeInteractive` strategy only takes \
                              effect there; outside the custom Document it is ignored, so the \
                              script no longer loads before the page becomes interactive.",
                severity: Severity::Error,
                doc_url: None,
                categories: &["nextjs"],
                skip_in_test_dir: false,
                skip_in_relaxed_dir: false,
            },
            "nextjs/no-before-interactive-script-outside-document",
            TS_FAMILY,
        ),
        oxlint_delegate(
            RuleMeta {
                id: "next-no-unwanted-polyfillio",
                description: "Don't load polyfills from polyfill.io.",
                remediation: "Replace the polyfill.io script with a trusted source \
                              (e.g. https://cdnjs.cloudflare.com/polyfill/) or rely on modern \
                              browser features directly. polyfill.io was compromised in a 2024 \
                              supply-chain attack, and many of the polyfills it serves are \
                              already shipped by Next.js, so the request is both unsafe and \
                              redundant.",
                severity: Severity::Warning,
                doc_url: None,
                categories: &["nextjs"],
                skip_in_test_dir: false,
                skip_in_relaxed_dir: false,
            },
            "nextjs/no-unwanted-polyfillio",
            TS_FAMILY,
        ),
    ]
}
