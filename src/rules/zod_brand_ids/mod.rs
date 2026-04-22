//! zod-brand-ids — nudge ID-like string fields toward `z.string().brand<"...">()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-brand-ids",
    description: "ID-like fields (`id`, `userId`, `post_id`) benefit from \
                  `.brand<\"...\">()` so distinct IDs are not assignable to \
                  each other.",
    remediation: "Chain `.brand<\"UserId\">()` (or a matching brand tag) \
                  onto the schema: `z.string().uuid().brand<\"UserId\">()`. \
                  Then `type UserId = z.infer<typeof userId>` produces a \
                  nominal type so a `PostId` cannot be passed where a \
                  `UserId` is expected.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
