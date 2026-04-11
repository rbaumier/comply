//! react-no-object-in-dep-array — flag non-primitive values in hook
//! dependency arrays.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-object-in-dep-array",
    description: "Hook dependency arrays must contain primitives, not objects/arrays.",
    remediation: "Extract the primitive field you depend on: \
                  `useEffect(() => { ... }, [user.id])` instead of `[user]`. \
                  Objects change reference on every render even when their \
                  content is identical, causing infinite re-runs.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript", "react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
