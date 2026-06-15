//! react-no-prop-assignments — disallow mutating a React component's props.
//!
//! React props are immutable inputs. Assigning to a member of the component's
//! first parameter (`props.bar = …`) mutates shared state owned by the parent
//! and is silently ignored or buggy under concurrent rendering. The component's
//! first parameter is recognised when the enclosing function returns JSX or is
//! the callback wrapped by `memo`/`forwardRef`.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "react-no-prop-assignments",
    description: "Mutating a React component's props is not allowed — props are immutable inputs.",
    remediation: "Copy the value into a local variable and mutate that instead, \
                  or lift the state into the parent and pass a new prop value.",
    severity: Severity::Error,
    doc_url: Some("https://biomejs.dev/linter/rules/no-react-prop-assignments/"),
    categories: &["react"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
        ],
    }
}
