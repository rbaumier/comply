mod react;
use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-derived-state-in-effect",
    description: "`useEffect` whose body only calls a state setter derives state — move the derivation to render.",
    remediation: "Replace `useEffect(() => { setX(a + b) }, [a, b])` with `const x = a + b` computed directly during render.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
