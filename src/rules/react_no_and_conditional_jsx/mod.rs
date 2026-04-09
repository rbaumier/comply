//! react-no-and-conditional-jsx — prefer ternary over && for conditional rendering.

mod typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-and-conditional-jsx",
    description: "`&&` renders 0/'' when the left operand is falsy-but-not-false.",
    remediation: "Replace `{expr && <X />}` with `{expr ? <X /> : null}` \
                  or `{Boolean(expr) && <X />}`. `&&` lets falsy values \
                  like `0` and `''` leak into the DOM.",
    severity: Severity::Warning,
    doc_url: None,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![(Language::Tsx, Backend::TreeSitter(Box::new(typescript::Check)))],
    }
}
