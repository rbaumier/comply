//! ts-no-generated-shape-redeclaration — flag a hand-written `type`/`interface`
//! whose every field a generated type of the project already carries.
//!
//! The generator owns the contract; a second declaration of the same fields is
//! a copy the next regeneration invalidates on one side only. Field types do
//! not have to agree — a name-for-name subset is already the copy, and it is
//! the one that keeps compiling after the column it mirrors changes type.
//!
//! Two conditions keep a name coincidence from reading as a copy. The
//! declaration has to be exported: a module-private shape is the use-site
//! narrowing the remediation asks for, and no second file can drift from it.
//! And exactly one generated contract may carry the names: `{ id, name }` sits
//! on every table of a schema, so finding it there points at no one table.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "ts-no-generated-shape-redeclaration",
    description: "A hand-written object type whose every field a generated type already \
                  carries redeclares a contract the generator owns.",
    remediation: "Read the generated type at the use site (`components['schemas']['X']`, \
                  `Tables<'t'>['Row']`) and narrow it there; an intermediate alias is a \
                  second copy.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["typescript"],

    skip_in_test_dir: true,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
