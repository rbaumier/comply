//! no-unnecessary-await — flag `await` on non-promise values.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "no-unnecessary-await",
    description: "Do not `await` non-promise values.",
    remediation: "Remove the unnecessary `await` — literals, arrays, functions, \
                  and other non-thenable values resolve synchronously and the \
                  `await` just adds confusion.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["unicorn"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
