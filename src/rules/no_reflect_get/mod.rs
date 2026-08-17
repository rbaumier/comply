//! no-reflect-get — flag `Reflect.get(...)` outside a forwarding `Proxy` trap.
//!
//! `Reflect.get(o, k)` reads a property while dropping its name from the type
//! system: the result is `any` for a dynamic key, and no annotation or `as`
//! remains for `ts-no-explicit-any` and `no-type-assertion` to catch.
//!
//! A `get` trap that forwards its own key parameter is exempt — inside a trap,
//! `Reflect.get(target, prop, receiver)` is the correct forwarding call and has
//! no typed alternative. The test is the key's identity, not the handler's
//! shape, so an inline `new Proxy(t, { get() {} })` literal, a detached one, a
//! `ProxyHandler<T>`-annotated one and a class trap all resolve alike.

mod oxc_typescript;

use crate::diagnostic::Severity;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;
use crate::rules::{RuleDef, TS_FAMILY};

pub const META: RuleMeta = RuleMeta {
    id: "no-reflect-get",
    description: "`Reflect.get` reads a property without its name reaching the type system.",
    remediation: "Read the property on a typed value, or parse the dynamic input into a named domain type at its boundary.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["type-safety"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: TS_FAMILY
            .iter()
            .map(|&lang| (lang, Backend::Oxc(Box::new(oxc_typescript::Check))))
            .collect(),
    }
}
