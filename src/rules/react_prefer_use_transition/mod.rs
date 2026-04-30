mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-prefer-use-transition",
    description: "Replace manual `loading` state with `useTransition` for concurrent-safe async UI.",
    remediation: "Replace `const [loading, setLoading] = useState(false)` + manual setLoading calls with `const [isPending, startTransition] = useTransition()`.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
