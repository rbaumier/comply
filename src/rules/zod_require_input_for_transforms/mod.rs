//! zod-require-input-for-transforms — prefer `z.input` when deriving a type
//! from a schema that applies `.transform()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "zod-require-input-for-transforms",
    description: "`z.infer<typeof Schema>` returns the *output* type of a schema. \
                  For schemas that use `.transform()`, the input shape (what the user \
                  actually types into a form) differs from the output.",
    remediation: "Use `z.input<typeof Schema>` for form values and `z.output<typeof Schema>` \
                  (or `z.infer`) for the parsed result.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["zod"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
