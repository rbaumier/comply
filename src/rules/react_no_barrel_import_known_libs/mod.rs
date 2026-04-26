//! react-no-barrel-import-known-libs — barrel (root) imports from icon/UI/util packages.

mod react;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-barrel-import-known-libs",
    description: "Named imports from known barrel packages (lucide-react, @mui/material, \
                  @mui/icons-material, react-icons, lodash, date-fns) pull the whole library \
                  into the bundle.",
    remediation: "Import from the library's subpath (e.g. `lodash/debounce`, \
                  `@mui/material/Button`, `lucide-react/icons/Check`) so bundlers \
                  can tree-shake effectively.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["react", "imports"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
