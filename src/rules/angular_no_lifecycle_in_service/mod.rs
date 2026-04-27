//! angular-no-lifecycle-in-service — services don't have component lifecycle hooks.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "angular-no-lifecycle-in-service",
    description: "Component lifecycle hooks like `ngOnInit` are never invoked on `@Injectable()` services.",
    remediation: "Move initialization to the constructor or use `OnDestroy` only with `providedIn: 'root'`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["angular"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
