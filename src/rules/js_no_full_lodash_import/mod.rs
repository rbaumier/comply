//! js-no-full-lodash-import — `import _ from 'lodash'` pulls the full
//! library; prefer per-function subpath imports or `lodash-es` for
//! tree-shaking.

mod typescript;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "js-no-full-lodash-import",
    description: "Importing from `lodash` pulls the entire library — use a subpath like `lodash/map` or `lodash-es`.",
    remediation: "Replace `import _ from 'lodash'` / `import { map } from 'lodash'` with \
                  `import map from 'lodash/map'` or switch to `lodash-es` (which tree-shakes).",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["bundle-size"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, typescript)
}
