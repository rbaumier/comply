//! Custom lint rules — each rule is a `RuleDef` with per-language backends.
//!
//! A rule concept owns a stable `RuleMeta` (id, description, remediation,
//! severity) and a list of `(Language, Backend)` pairs. The engine walks
//! every registered rule, filters by the file's language, and dispatches
//! to the matching backend.
//!
//! Backends can be:
//! - `TreeSitter` — in-process Rust AST walk (the common case for opinionated rules)
//! - `Text` — plain-text / regex / filesystem check (line count, TODO scan)
//! - `Oxlint` — delegation to an oxlint rule, with rule-id + message remap
//! - `Clippy` — (v2) delegation to a clippy lint
//! - `Tsc` — (v1.2) shell out to `tsc --noEmit`
//!
//! See TODO.md "Architecture" for the full rationale.

pub mod backend;
pub mod banned_identifiers;
pub mod boolean_naming;
pub mod delegated;
pub mod drizzle_timestamp_with_timezone;
pub mod error_without_cause;
pub mod explicit_return_type_on_exported;
pub mod explicit_units;
pub mod exports_at_top;
pub mod jsdoc_on_exported;
pub mod law_of_demeter;
pub mod max_file_lines;
pub mod max_function_lines;
pub mod meta;
pub mod module_header;
pub mod no_abbreviated_names;
pub mod no_and_in_function_name;
pub mod no_auth_token_in_localstorage;
pub mod no_boolean_flag_param;
pub mod no_commented_out_code;
pub mod no_common_grab_bag;
pub mod no_dangerously_set_inner_html;
pub mod no_default_params;
pub mod no_double_cast;
pub mod no_enum;
pub mod no_focused_test;
pub mod no_function_overloads;
pub mod no_generic_names;
pub mod no_hardcoded_secret;
pub mod no_inline_param_type;
pub mod no_json_parse_cast;
pub mod no_match_snapshot;
pub mod no_multi_op_oneliner;
pub mod no_nested_ternary;
pub mod no_new_regex_with_variable;
pub mod no_nullish_default_on_input;
pub mod no_put_method;
pub mod no_set_x_to_y;
pub mod no_skipped_test_without_link;
pub mod no_throw;
pub mod no_type_encoded_names;
pub mod no_verb_in_rest_url;
pub mod prefer_switch_over_chained_if;
pub mod prefer_type_over_interface;
pub mod react_hoist_regex_outside_component;
pub mod rust_arc_non_send_sync;
pub mod rust_await_holding_lock;
pub mod rust_block_on_in_async;
pub mod rust_builder_without_must_use;
pub mod rust_explicit_iter_loop;
pub mod rust_helpers;
pub mod rust_impl_debug_on_public_types;
pub mod rust_large_enum_variant;
pub mod rust_mod_tests_without_cfg_test;
pub mod rust_must_use_on_result;
pub mod rust_no_bool_return_from_fallible;
pub mod rust_no_box_default;
pub mod rust_no_dbg_macro;
pub mod rust_no_empty_test_fn;
pub mod rust_no_float_for_money;
pub mod rust_no_format_in_debug_impl;
pub mod rust_no_large_tuple_return;
pub mod rust_no_linkedlist;
pub mod rust_no_lossy_as_cast;
pub mod rust_no_panic_macros;
pub mod rust_no_println_in_library;
pub mod rust_no_pub_use_glob;
pub mod rust_no_static_mut;
pub mod rust_no_unwrap;
pub mod rust_no_unwrap_in_from_impl;
pub mod rust_ptr_arg;
pub mod rust_pub_enum_without_non_exhaustive;
pub mod rust_rc_mutex;
pub mod rust_redundant_clone;
pub mod rust_serde_deny_unknown_fields;
pub mod rust_string_as_error;
pub mod rust_sync_io_in_async;
pub mod rust_thread_sleep_in_async;
pub mod rust_tokio_spawn_without_handle;
pub mod rust_unbounded_channel;
pub mod rust_undocumented_unsafe;
pub mod rust_unit_error_result;
pub mod rust_unsafe_impl_without_comment;
pub mod react_no_and_conditional_jsx;
pub mod react_no_array_index_key;
pub mod react_use_state_lazy_init;
pub mod tailwind_no_dynamic_class;
pub mod tanstack_query_array_key;
pub mod tanstack_query_no_deprecated_props;
pub mod timeout_on_io;
pub mod todo_needs_issue_link;
pub mod walker;
pub mod zod_no_any;
pub mod zod_prefer_top_level_format;

use crate::diagnostic::Severity;
use crate::files::Language;
use backend::Backend;
use meta::RuleMeta;

/// A rule: identity + per-language enforcement backends.
pub struct RuleDef {
    pub meta: RuleMeta,
    pub backends: Vec<(Language, Backend)>,
}

/// Language slice for the TS-family. Used by rules that apply to all three
/// variants identically (either via the TS grammar or oxlint delegation).
pub const TS_FAMILY: &[Language] = &[Language::TypeScript, Language::Tsx, Language::JavaScript];

/// Helper for rules whose enforcement is 100% delegated to oxlint.
/// Each entry in `languages` gets a `Backend::Oxlint { rule }` binding.
pub fn oxlint_delegate(meta: RuleMeta, rule: &'static str, languages: &[Language]) -> RuleDef {
    RuleDef {
        meta,
        backends: languages
            .iter()
            .map(|&lang| (lang, Backend::Oxlint { rule }))
            .collect(),
    }
}

/// Helper for rules bound to BOTH oxlint (TS-family) and clippy (Rust).
/// Used when the same coding standard has direct enforcement on both
/// sides: `max-params` → oxlint `max-params` + clippy `too_many_arguments`.
pub fn oxlint_and_clippy(
    meta: RuleMeta,
    oxlint_rule: &'static str,
    clippy_lint: &'static str,
) -> RuleDef {
    let mut backends: Vec<(Language, Backend)> = TS_FAMILY
        .iter()
        .map(|&lang| (lang, Backend::Oxlint { rule: oxlint_rule }))
        .collect();
    backends.push((Language::Rust, Backend::Clippy { lint: clippy_lint }));
    RuleDef { meta, backends }
}

/// Accessor for the oxlint-delegated backends across every registered rule.
/// Used by the oxlint subprocess module to generate the runtime config and
/// build the diagnostic-code remap table.
pub fn collect_oxlint_bindings() -> Vec<(&'static str, &'static RuleMeta, Severity)> {
    let mut bindings = Vec::new();
    for rule in all_rule_defs() {
        // Leak the meta to 'static so the caller can reference it across the
        // oxlint subprocess boundary without lifetime gymnastics. This runs
        // once per process invocation, so the leak is negligible.
        let meta_static: &'static RuleMeta = Box::leak(Box::new(rule.meta));
        for (_lang, backend) in &rule.backends {
            if let Backend::Oxlint { rule: oxlint_key } = backend {
                bindings.push((*oxlint_key, meta_static, meta_static.severity));
            }
        }
    }
    // Dedupe by oxlint config key (TS_FAMILY yields 3 bindings for the same key).
    bindings.sort_by_key(|(key, _, _)| *key);
    bindings.dedup_by_key(|(key, _, _)| *key);
    bindings
}

/// Accessor for the clippy-delegated backends across every registered rule.
/// Mirror of `collect_oxlint_bindings` but for `Backend::Clippy { lint }`
/// markers. Used by `crate::clippy` to generate the `-W clippy::lint`
/// flags passed to `cargo clippy` and to build the rule-id remap table.
pub fn collect_clippy_bindings() -> Vec<(&'static str, &'static RuleMeta, Severity)> {
    let mut bindings = Vec::new();
    for rule in all_rule_defs() {
        let meta_static: &'static RuleMeta = Box::leak(Box::new(rule.meta));
        for (_lang, backend) in &rule.backends {
            if let Backend::Clippy { lint } = backend {
                bindings.push((*lint, meta_static, meta_static.severity));
            }
        }
    }
    // Dedupe by lint name — multiple rules occasionally share a clippy
    // lint (e.g. `disallowed_names` is referenced by both
    // `banned_identifiers` and `no_generic_names`). Keep the first one.
    bindings.sort_by_key(|(lint, _, _)| *lint);
    bindings.dedup_by_key(|(lint, _, _)| *lint);
    bindings
}

/// All registered rules — both the custom ones and the oxlint-delegated ones.
pub fn all_rule_defs() -> Vec<RuleDef> {
    let mut rules = vec![
        max_file_lines::register(),
        max_function_lines::register(),
        no_throw::register(),
        no_nested_ternary::register(),
        banned_identifiers::register(),
        todo_needs_issue_link::register(),
        no_commented_out_code::register(),
        no_common_grab_bag::register(),
        no_default_params::register(),
        boolean_naming::register(),
        exports_at_top::register(),
        jsdoc_on_exported::register(),
        module_header::register(),
        no_boolean_flag_param::register(),
        explicit_units::register(),
        no_abbreviated_names::register(),
        no_generic_names::register(),
        no_type_encoded_names::register(),
        law_of_demeter::register(),
        timeout_on_io::register(),
        no_nullish_default_on_input::register(),
        prefer_switch_over_chained_if::register(),
        no_multi_op_oneliner::register(),
        // v1.2 — api-design + language-typescript rules
        no_enum::register(),
        no_double_cast::register(),
        no_json_parse_cast::register(),
        explicit_return_type_on_exported::register(),
        no_inline_param_type::register(),
        prefer_type_over_interface::register(),
        no_function_overloads::register(),
        no_verb_in_rest_url::register(),
        no_put_method::register(),
        // v1.4 — ecosystem rules (security / testing / react / tanstack / zod / drizzle / tailwind)
        no_new_regex_with_variable::register(),
        no_auth_token_in_localstorage::register(),
        no_dangerously_set_inner_html::register(),
        no_hardcoded_secret::register(),
        no_focused_test::register(),
        no_skipped_test_without_link::register(),
        no_match_snapshot::register(),
        react_no_array_index_key::register(),
        react_use_state_lazy_init::register(),
        react_no_and_conditional_jsx::register(),
        react_hoist_regex_outside_component::register(),
        tanstack_query_array_key::register(),
        tanstack_query_no_deprecated_props::register(),
        zod_prefer_top_level_format::register(),
        zod_no_any::register(),
        drizzle_timestamp_with_timezone::register(),
        tailwind_no_dynamic_class::register(),
        // v1.5 — Rust rules from the language-rust skill. All have clippy
        // coverage; these mod.rs files document them so `comply list` and
        // `comply explain` surface the mapping. See each rule's rust.rs
        // for the corresponding clippy lint name + setup.
        rust_no_unwrap::register(),
        rust_no_panic_macros::register(),
        rust_must_use_on_result::register(),
        rust_undocumented_unsafe::register(),
        rust_no_println_in_library::register(),
        rust_await_holding_lock::register(),
        rust_large_enum_variant::register(),
        rust_ptr_arg::register(),
        rust_explicit_iter_loop::register(),
        rust_no_linkedlist::register(),
        rust_redundant_clone::register(),
        // v2.0 — Rust-native custom rules (not mere clippy markers).
        rust_rc_mutex::register(),
        // v2.1 — More Rust-native rules: high-signal runtime bugs +
        // a couple of doc-only markers for clippy lints in the same family.
        rust_no_static_mut::register(),
        rust_unit_error_result::register(),
        rust_no_float_for_money::register(),
        rust_unbounded_channel::register(),
        rust_thread_sleep_in_async::register(),
        rust_block_on_in_async::register(),
        rust_sync_io_in_async::register(),
        rust_serde_deny_unknown_fields::register(),
        rust_builder_without_must_use::register(),
        rust_arc_non_send_sync::register(),
        rust_no_box_default::register(),
        // v2.2 — Rust-native rules: debugging hygiene, error typing,
        // public type discoverability, test gating.
        rust_no_dbg_macro::register(),
        rust_tokio_spawn_without_handle::register(),
        rust_string_as_error::register(),
        rust_impl_debug_on_public_types::register(),
        rust_mod_tests_without_cfg_test::register(),
        rust_no_bool_return_from_fallible::register(),
        rust_no_large_tuple_return::register(),
        rust_unsafe_impl_without_comment::register(),
        // v2.3 — final batch: API hygiene + safety rules.
        rust_no_unwrap_in_from_impl::register(),
        rust_pub_enum_without_non_exhaustive::register(),
        rust_no_pub_use_glob::register(),
        rust_no_lossy_as_cast::register(),
        rust_no_format_in_debug_impl::register(),
        rust_no_empty_test_fn::register(),
        // v2.7 — Cat A: mechanical AST rules from the coding-standards skill.
        error_without_cause::register(),
        no_set_x_to_y::register(),
        no_and_in_function_name::register(),
    ];
    rules.extend(delegated::register_all());
    rules
}
