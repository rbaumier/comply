//! react-use-lazy-motion — importing `motion` from `framer-motion` ships
//! the full animation engine; `LazyMotion` + `m` lazy-loads features and
//! saves ~30kB.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-use-lazy-motion",
    description: "Importing `motion` from `framer-motion` — use `LazyMotion` + `m` for a \
                  smaller bundle.",
    remediation: "Wrap your tree in `<LazyMotion features={domAnimation}>` and replace \
                  `motion.div` with `m.div` to lazy-load animation features.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["bundle-size"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
