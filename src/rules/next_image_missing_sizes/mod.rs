//! next-image-missing-sizes — `<Image fill />` without `sizes` makes the
//! browser download the largest source. Always pair `fill` with `sizes`.

mod react;

use crate::diagnostic::Severity;
use crate::rules::RuleDef;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "next-image-missing-sizes",
    description: "`next/image` `<Image fill />` without a `sizes` prop forces the browser to download the largest source.",
    remediation: "Add a `sizes` attribute, e.g. `sizes=\"(max-width: 768px) 100vw, 50vw\"`. \
                  Without `sizes`, `next/image` falls back to `100vw` and serves the largest \
                  image in the `srcset`, blowing the LCP budget on smaller viewports.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["nextjs"],
};

pub fn register() -> RuleDef {
    crate::register_ts_family!(META, react)
}
