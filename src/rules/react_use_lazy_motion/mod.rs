//! react-use-lazy-motion — importing `motion` from `framer-motion` ships
//! the full animation engine; `LazyMotion` + `m` lazy-loads features and
//! saves ~30kB.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-use-lazy-motion",
    description: "Importing `motion` from `framer-motion` — use `LazyMotion` + `m` for a \
                  smaller bundle.",
    remediation: "Wrap your tree in `<LazyMotion features={domAnimation}>` and replace \
                  `motion.div` with `m.div` to lazy-load animation features.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["bundle-size"],

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
