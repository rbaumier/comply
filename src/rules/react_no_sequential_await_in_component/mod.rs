//! react-no-sequential-await-in-component — parallelise component data loads.

mod oxc_typescript;
#[cfg(test)]
mod react;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-sequential-await-in-component",
    description: "Sequential `await` of independent calls inside an async React \
                  component serialises fetches that could run in parallel.",
    remediation: "Wrap independent awaits in `Promise.all([...])`. Example: \
                  `const [user, posts] = await Promise.all([getUser(id), getPosts(id)])`. \
                  Server Components block rendering on each await, so chaining \
                  two fetches doubles the latency.",
    severity: Severity::Warning,
    doc_url: Some(
        "https://react.dev/reference/rsc/server-components#async-components-with-server-components",
    ),
    categories: &["react"],
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
