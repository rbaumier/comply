//! nestjs-no-missing-injectable — provider classes need `@Injectable()`.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "nestjs-no-missing-injectable",
    description: "Classes named `*Service` / `*Repository` used as providers must have `@Injectable()`.",
    remediation: "Add `@Injectable()` from `@nestjs/common` to the class declaration.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nestjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
