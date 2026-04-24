//! api-no-internal-ids-in-response — flag response-shaped interfaces/
//! type-aliases that expose internal column names (`*_id`, `internal_*`,
//! `pk`, `rowid`). Leaking DB shape into the public surface couples the
//! wire format to schema choices and traps future migrations.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "api-no-internal-ids-in-response",
    description:
        "Response DTOs must not expose internal column names, sequential IDs, or implementation fields.",
    remediation:
        "Rename the field to its public counterpart (e.g. `user_id` → `userId`, `pk` → `id`) and drop implementation-only columns from the response shape.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["api-design"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
