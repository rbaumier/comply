//! zod-brand-ids — nudge ID-like string fields toward `z.string().brand<"...">()`.

mod oxc_typescript;
#[cfg(test)]
mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

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
