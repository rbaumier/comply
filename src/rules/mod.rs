//! Custom lint rules — each rule is a `RuleDef` with per-language backends.
//!
//! A rule concept owns a stable `RuleMeta` (id, description, remediation,
//! severity) and a list of `(Language, Backend)` pairs. The engine walks
//! every registered rule, filters by the file's language, and dispatches
//! to the matching backend.
//!
//! Backends can be:
//! - `TreeSitter` — in-process Rust AST walk (the common case for opinionated rules)
//! - `Text` — plain-text / regex / filesystem check (line count, TODO scan) // comply-ignore: todo-needs-issue-link — mention, not marker.
//! - `Oxlint` — delegation to an oxlint rule, with rule-id + message remap
//! - `Clippy` — (v2) delegation to a clippy lint
//! - `Tsc` — (v1.2) shell out to `tsc --noEmit`
//!
//! See TODO.md "Architecture" for the full rationale. // comply-ignore: todo-needs-issue-link — file reference, not marker.

pub mod angular_no_topromise;
pub mod angular_prefer_signals;
pub mod angular_require_onpush;
pub mod api_branded_id_types;
pub mod api_no_internal_ids_in_response;
pub mod api_no_nullable_variant_fields;
pub mod api_put_vs_patch;
pub mod api_route_version_prefix;
pub mod api_separate_input_output_types;
pub mod api_validate_at_boundaries;
pub mod arrow_this_in_function;
pub mod backend;
pub mod ban_dependencies;
pub mod banned_comment_words;
pub mod better_auth_client_framework_import;
pub mod better_auth_drizzle_useplural;
pub mod better_auth_email_verification_handler;
pub mod better_auth_expo_no_cookie_auth;
pub mod better_auth_no_disable_csrf;
pub mod better_auth_no_disable_origin_check;
pub mod better_auth_no_duplicate_baseurl;
pub mod better_auth_no_duplicate_secret;
pub mod better_auth_plugin_import_path;
pub mod better_auth_require_rate_limit;
pub mod better_auth_required_user_fields;
pub mod better_auth_reset_password_handler;
pub mod better_auth_secret_min_length;
pub mod better_auth_session_infer_type;
pub mod better_auth_trusted_providers;
pub mod better_result_await_inside_gen;
pub mod better_result_caller_must_handle;
pub mod better_result_catch_returns_tagged;
pub mod better_result_constructor_spreads_args;
pub mod better_result_no_catch_panic;
pub mod better_result_no_manual_propagation;
pub mod better_result_no_mixed_throw;
pub mod better_result_no_nullable_return;
pub mod better_result_no_param_properties;
pub mod better_result_no_promise_catch;
pub mod better_result_no_rewrap_error;
pub mod better_result_no_throw;
pub mod better_result_no_try_catch;
pub mod better_result_prefer_map_single;
pub mod better_result_prefer_matcherror_exhaustive;
pub mod better_result_require_gen_for_chains;
pub mod better_result_tag_matches_classname;
pub mod better_result_tagged_error_cause_unknown;
pub mod better_result_tagged_error_message;
pub mod better_result_try_requires_catch;
pub mod block_scope_case;
pub mod boolean_naming;
pub mod boundary_condition;
pub mod call_expression;
pub mod ci_cache_key_includes_lockfile;
pub mod ci_checkout_action_pinned;
pub mod ci_docker_gha_cache;
pub mod ci_no_hardcoded_db_password;
pub mod ci_no_plaintext_secrets;
pub mod ci_playwright_report_upload;
pub mod ci_postgres_healthcheck;
pub mod ci_setup_node_cache_enabled;
pub mod ci_use_npm_ci;
pub mod comment_max_words;
pub mod comment_paraphrases_code;
pub mod compose_bind_localhost_ports;
pub mod compose_cap_drop_all;
pub mod compose_depends_on_condition;
pub mod compose_healthcheck_required;
pub mod compose_no_inline_secrets;
pub mod compose_no_latest_tag;
pub mod compose_no_network_host;
pub mod compose_no_privileged;
pub mod compose_require_resource_limits;
pub mod css_calc_needs_spaces;
pub mod css_custom_property_needs_var;
pub mod css_font_family_needs_generic;
pub mod css_font_family_quotes;
pub mod css_keyframe_no_duplicate_selectors;
pub mod css_keyframe_no_important;
pub mod css_no_deprecated_at_rule;
pub mod css_no_deprecated_media_type;
pub mod css_no_deprecated_property_value;
pub mod css_no_duplicate_custom_properties;
pub mod css_no_duplicate_font_family;
pub mod css_no_duplicate_imports;
pub mod css_no_duplicate_properties;
pub mod css_no_empty_block;
pub mod css_no_empty_comment;
pub mod css_no_invalid_hex;
pub mod css_no_invalid_media_query;
pub mod css_no_nonstandard_gradient_direction;
pub mod css_no_redundant_longhand;
pub mod css_no_shorthand_overrides_longhand;
pub mod css_no_unknown_function;
pub mod css_no_unknown_media_feature;
pub mod css_no_unknown_media_value;
pub mod css_no_unknown_property_value;
pub mod css_no_vendor_prefix_at_rule;
pub mod css_no_vendor_prefix_media;
pub mod css_no_vendor_prefix_property;
pub mod css_no_vendor_prefix_selector;
pub mod css_no_vendor_prefix_value;
pub mod css_outline_none_needs_focus;
pub mod db_no_n_plus_one;
pub mod db_no_string_concat_sql;
pub mod delegated;
pub mod dockerfile_absolute_workdir;
pub mod dockerfile_add_for_archive_extract;
pub mod dockerfile_apk_no_cache;
pub mod dockerfile_apt_clean_lists;
pub mod dockerfile_apt_get_y_flag;
pub mod dockerfile_apt_no_recommends;
pub mod dockerfile_copy_after_install;
pub mod dockerfile_copy_from_known_stage;
pub mod dockerfile_copy_from_not_self;
pub mod dockerfile_copy_needs_workdir;
pub mod dockerfile_copy_trailing_slash;
pub mod dockerfile_dnf_clean_all;
pub mod dockerfile_dnf_y_flag;
pub mod dockerfile_env_no_self_reference;
pub mod dockerfile_exec_form_cmd;
pub mod dockerfile_instruction_order;
pub mod dockerfile_label_not_empty;
pub mod dockerfile_label_url_format;
pub mod dockerfile_no_add_for_files;
pub mod dockerfile_no_apt_end_user;
pub mod dockerfile_no_cd_in_run;
pub mod dockerfile_no_curl_and_wget;
pub mod dockerfile_no_from_platform;
pub mod dockerfile_no_latest_tag;
pub mod dockerfile_no_maintainer;
pub mod dockerfile_no_multiple_cmd;
pub mod dockerfile_no_multiple_entrypoint;
pub mod dockerfile_no_onbuild_recursion;
pub mod dockerfile_no_secrets_in_arg;
pub mod dockerfile_no_secrets_in_copy;
pub mod dockerfile_no_secrets_in_env;
pub mod dockerfile_no_shell_utils_in_run;
pub mod dockerfile_no_sudo;
pub mod dockerfile_no_zypper_dist_upgrade;
pub mod dockerfile_pin_exact_version;
pub mod dockerfile_pip_no_cache_dir;
pub mod dockerfile_pipefail;
pub mod dockerfile_require_dockerignore;
pub mod dockerfile_require_healthcheck;
pub mod dockerfile_require_multi_stage;
pub mod dockerfile_require_non_root_user;
pub mod dockerfile_single_healthcheck;
pub mod dockerfile_unique_stage_names;
pub mod dockerfile_use_cache_mount;
pub mod dockerfile_use_frozen_lockfile;
pub mod dockerfile_use_npm_ci;
pub mod dockerfile_useradd_low_uid;
pub mod dockerfile_valid_port;
pub mod dockerfile_wget_progress_flag;
pub mod dockerfile_yarn_cache_clean;
pub mod dockerfile_yum_clean_all;
pub mod dockerfile_yum_y_flag;
pub mod dockerfile_zypper_clean;
pub mod dockerfile_zypper_non_interactive;
pub mod dockerignore_must_exclude_sensitive;
pub mod drizzle_camel_snake_column_names;
pub mod drizzle_chunk_large_batch_insert;
pub mod drizzle_config_satisfies;
pub mod drizzle_consistent_table_naming;
pub mod drizzle_created_at_default_now;
pub mod drizzle_fk_needs_index;
pub mod drizzle_json_requires_type;
pub mod drizzle_junction_composite_pk;
pub mod drizzle_multi_statement_tx;
pub mod drizzle_no_new_pool_per_request;
pub mod drizzle_no_select_without_limit;
pub mod drizzle_no_sql_raw_with_variable;
pub mod drizzle_pool_requires_timeouts;
pub mod drizzle_prefer_findmany_relations;
pub mod drizzle_prefer_inarray;
pub mod drizzle_prefer_infer_select;
pub mod drizzle_prepared_placeholder;
pub mod drizzle_returning_on_insert_update;
pub mod drizzle_serverless_pool_max_one;
pub mod drizzle_soft_delete_filter;
pub mod drizzle_timestamp_with_timezone;
pub mod drizzle_updated_at_on_update;
pub mod drizzle_zod_omit_generated;
pub mod drizzle_zod_prefer_generated_schema;
pub mod elysia_after_response_mutation;
pub mod elysia_apollo_no_req_res;
pub mod elysia_apollo_playground_prod;
pub mod elysia_array_no_bounds;
pub mod elysia_bearer_missing_www_auth;
pub mod elysia_bearer_not_validated;
pub mod elysia_bearer_strip_typo;
pub mod elysia_better_auth_basepath;
pub mod elysia_better_auth_mount;
pub mod elysia_better_auth_null_session;
pub mod elysia_booleanstring_for_body;
pub mod elysia_cf_compile_required;
pub mod elysia_cf_env_import;
pub mod elysia_cf_no_inline_values;
pub mod elysia_cf_no_static_plugin;
pub mod elysia_cookie_getter_setter;
pub mod elysia_cookie_no_httponly;
pub mod elysia_cookie_no_samesite;
pub mod elysia_cookie_no_secure;
pub mod elysia_cookie_removal_api;
pub mod elysia_cookie_signed_no_secrets;
pub mod elysia_cors_allowed_headers_wildcard;
pub mod elysia_cors_credentials_wildcard;
pub mod elysia_cors_methods_wildcard;
pub mod elysia_cors_regex_unanchored;
pub mod elysia_cors_wildcard;
pub mod elysia_cron_name_required;
pub mod elysia_cron_timezone;
pub mod elysia_custom_errors_in_model;
pub mod elysia_deno_serve_fetch;
pub mod elysia_deploy_no_graceful_shutdown;
pub mod elysia_deploy_no_health;
pub mod elysia_deploy_port_hardcoded;
pub mod elysia_deploy_prod_no_aot;
pub mod elysia_derive_validated_data;
pub mod elysia_drizzle_intermediate_var;
pub mod elysia_eden_error_unchecked;
pub mod elysia_eden_null_body;
pub mod elysia_eden_server_export_type;
pub mod elysia_file_magic_number;
pub mod elysia_file_upload_no_maxsize;
pub mod elysia_file_upload_no_type;
pub mod elysia_global_with_types;
pub mod elysia_graphql_yoga_context;
pub mod elysia_group_deep_paths;
pub mod elysia_guard_derive_no_headers;
pub mod elysia_headers_lowercase;
pub mod elysia_heavy_onrequest;
pub mod elysia_hooks_before_routes;
pub mod elysia_html_import_uppercase;
pub mod elysia_html_xss_no_safe;
pub mod elysia_import_t_not_typebox;
pub mod elysia_inline_handlers;
pub mod elysia_jwt_cookie_no_httponly;
pub mod elysia_jwt_missing_exp;
pub mod elysia_jwt_name_multiple;
pub mod elysia_jwt_secret_hardcoded;
pub mod elysia_jwt_verify_unchecked;
pub mod elysia_listen_callback_info;
pub mod elysia_listen_port_type;
pub mod elysia_macro_named_inference;
pub mod elysia_macro_throw_status;
pub mod elysia_mapresponse_sync_compression;
pub mod elysia_model_export_types;
pub mod elysia_named_plugin;
pub mod elysia_nextjs_typeof_process;
pub mod elysia_no_body_on_get;
pub mod elysia_no_context_type;
pub mod elysia_no_mix_zod_typebox;
pub mod elysia_no_server_assertion;
pub mod elysia_nodejs_adapter_required;
pub mod elysia_numeric_body_no_coerce;
pub mod elysia_numeric_no_bounds;
pub mod elysia_objectstring_for_query;
pub mod elysia_onerror_missing_validation;
pub mod elysia_onparse_no_content_type;
pub mod elysia_openapi_from_types_prod;
pub mod elysia_openapi_security_scheme;
pub mod elysia_otel_named_functions;
pub mod elysia_plugin_functional_callback;
pub mod elysia_prefer_instance_plugin;
pub mod elysia_prefer_redirect;
pub mod elysia_prefer_status_over_set;
pub mod elysia_require_method_chaining;
pub mod elysia_resolve_outside_guard;
pub mod elysia_response_keyed_by_status;
pub mod elysia_response_status_mismatch;
pub mod elysia_route_all_method;
pub mod elysia_route_missing_auth;
pub mod elysia_route_missing_body_schema;
pub mod elysia_route_missing_params_schema;
pub mod elysia_route_missing_response_schema;
pub mod elysia_scope_missing;
pub mod elysia_server_timing_prod;
pub mod elysia_service_coupled;
pub mod elysia_service_return_not_throw;
pub mod elysia_static_await_hmr;
pub mod elysia_static_inline_value;
pub mod elysia_streaming_headers_after_yield;
pub mod elysia_string_format_email;
pub mod elysia_test_listen_not_handle;
pub mod elysia_test_missing_401;
pub mod elysia_test_missing_validation;
pub mod elysia_transform_no_schema;
pub mod elysia_ws_connection_leak;
pub mod elysia_ws_headers_unvalidated;
pub mod elysia_ws_missing_auth;
pub mod elysia_ws_subscribe_before_publish;
pub mod elysia_zod_coerce_params;
pub mod enforce_delete_with_where;
pub mod enforce_update_with_where;
pub mod error_without_cause;
pub mod exception_use_error_cause;
pub mod explicit_units;
pub mod file_ctx;
pub mod path_utils;
pub mod function_doc_banned_verbs;
pub mod i18n_key_exists;
pub mod i18n_key_requires_domain_prefix;
pub mod i18n_max_key_depth;
pub mod i18n_no_english_key;
pub mod i18n_no_manual_list_join;
pub mod i18n_use_singleton_outside_react;
pub mod jsdoc_check_property_names;
pub mod jsdoc_check_tag_names;
pub mod jsdoc_check_template_names;
pub mod jsdoc_check_types;
pub mod jsdoc_check_values;
pub mod jsdoc_helpers;
pub mod jsdoc_missing_example;
pub mod jsdoc_require_hyphen_before_param_description;
pub mod jsdoc_require_next_description;
pub mod jsdoc_require_param_description;
pub mod jsdoc_require_param_name;
pub mod jsdoc_require_property;
pub mod jsdoc_require_property_description;
pub mod jsdoc_require_property_name;
pub mod jsdoc_require_rejects;
pub mod jsdoc_require_returns_description;
pub mod jsdoc_require_tags;
pub mod jsdoc_require_template;
pub mod jsdoc_require_template_description;
pub mod jsdoc_require_yields;
pub mod jsdoc_require_yields_check;
pub mod jsdoc_require_yields_description;
pub mod jsdoc_text_helpers;
pub mod jsdoc_valid_types;
pub mod jsx;
pub mod k8s_dangling_hpa;
pub mod k8s_dangling_ingress;
pub mod k8s_dangling_network_policy;
pub mod k8s_dangling_network_policy_peer;
pub mod k8s_dangling_service;
pub mod k8s_dangling_service_monitor;
pub mod k8s_deployment_anti_affinity;
pub mod k8s_disallow_privilege_escalation;
pub mod k8s_dnsconfig_options;
pub mod k8s_env_value_from_resolves;
pub mod k8s_hpa_min_three_replicas;
pub mod k8s_invalid_target_ports;
pub mod k8s_job_ttl_required;
pub mod k8s_min_replicas_two;
pub mod k8s_mismatching_selector;
pub mod k8s_no_allow_privileged_scc;
pub mod k8s_no_default_service_account;
pub mod k8s_no_deprecated_extensions_api;
pub mod k8s_no_deprecated_service_account_field;
pub mod k8s_no_docker_sock_mount;
pub mod k8s_no_duplicate_env_vars;
pub mod k8s_no_exposed_services;
pub mod k8s_no_host_ipc;
pub mod k8s_no_host_network;
pub mod k8s_no_host_pid;
pub mod k8s_no_latest_image_tag;
pub mod k8s_no_plaintext_secret_in_git;
pub mod k8s_no_privileged_container;
pub mod k8s_no_privileged_ports;
pub mod k8s_no_secret_in_env_literal;
pub mod k8s_no_secrets_in_configmap;
pub mod k8s_no_sensitive_host_mounts;
pub mod k8s_no_ssh_port;
pub mod k8s_no_unsafe_proc_mount;
pub mod k8s_no_unsafe_sysctls;
pub mod k8s_no_writable_host_mount;
pub mod k8s_non_existent_service_account;
pub mod k8s_pdb_eviction_policy;
pub mod k8s_prefer_secret_files_over_env;
pub mod k8s_priority_class_name;
pub mod k8s_probe_port_exists;
pub mod k8s_rbac_no_cluster_admin_binding;
pub mod k8s_rbac_no_create_pods;
pub mod k8s_rbac_no_secret_access;
pub mod k8s_rbac_no_wildcard_resources;
pub mod k8s_rbac_no_wildcard_verbs;
pub mod k8s_require_drop_all_caps;
pub mod k8s_require_explicit_namespace;
pub mod k8s_require_ingress_tls;
pub mod k8s_require_liveness_probe;
pub mod k8s_require_network_policy;
pub mod k8s_require_pod_disruption_budget;
pub mod k8s_require_read_only_root;
pub mod k8s_require_readiness_probe;
pub mod k8s_require_resource_limits;
pub mod k8s_require_resource_requests;
pub mod k8s_require_run_as_non_root;
pub mod k8s_require_standard_labels;
pub mod k8s_restart_policy_required;
pub mod k8s_rolling_update_zero_unavailable;
pub mod meta;
pub mod migration_needs_lock_timeout;
pub mod migration_needs_rollback;
pub mod no_history_in_comments;
pub mod no_shallow_passthrough_method;
pub mod perf_font_face_display_swap;
pub mod perf_font_preload_crossorigin;
pub mod perf_img_fetchpriority_high;
pub mod perf_img_modern_format;
pub mod perf_no_google_fonts_link;
pub mod perf_no_render_blocking_css;
pub mod perf_prefers_reduced_motion;
pub mod perf_route_level_code_split;
pub mod pg_require_limit;
pub mod post_message_origin;
pub mod prisma_no_findmany_without_take;
pub mod prisma_no_nested_include_depth;
pub mod prisma_require_transaction_for_multi_write;
pub mod react_no_barrel_import_known_libs;
pub mod react_no_blocking_log_after_mutation;
pub mod react_no_boolean_variant_props;
pub mod react_no_chained_filter_map_reduce;
pub mod react_no_dedup_filter_indexof;
pub mod react_no_destructure_zustand_store;
pub mod react_no_find_in_map_loop;
pub mod react_no_interleaved_layout_rw;
pub mod react_no_setstate_without_updater;
pub mod react_no_sort_for_extrema;
pub mod react_no_unwrapped_localstorage;
pub mod react_no_use_client_without_client_api;
pub mod react_no_usestate_high_frequency;
pub mod react_require_content_visibility;
pub mod react_require_versioned_storage_key;
pub mod rn_auth_token_securestore;
pub mod rn_biometrics_hardware_check;
pub mod rn_expo_router_layout_required;
pub mod rn_flashlist_estimated_item_size;
pub mod rn_flashlist_over_flatlist;
pub mod rn_image_source_object;
pub mod rn_memo_list_items;
pub mod rn_no_inline_renderitem;
pub mod rn_no_inline_styles;
pub mod rn_no_react_navigation_stack;
pub mod rn_no_string_route_names;
pub mod rn_push_permissions_before_token;
pub mod rn_push_token_requires_projectid;
pub mod rn_raw_string_in_text;
pub mod rn_reanimated_over_animated;
pub mod rn_router_replace_after_login;
pub mod rust_asref_path_for_fs_fns;
pub mod rust_no_arc_mutex_tree;
pub mod rust_no_println_in_async;
pub mod rust_workspace_deps_centralized;
pub mod rust_workspace_lints_shared;
pub mod security_bcrypt_min_rounds;
pub mod security_cookie_no_samesite_none;
pub mod security_no_cors_reflect_origin;
pub mod security_no_deserialize_untrusted;
pub mod security_no_password_in_log;
pub mod security_no_query_without_ownership;
pub mod security_no_sri_missing;
pub mod security_require_helmet;
pub mod security_require_hsts;
pub mod security_require_oauth_state;
pub mod security_require_pkce_oauth;
pub mod security_require_rate_limit_auth;
pub mod shadcn_avatar_requires_fallback;
pub mod shadcn_button_icon_data_attr;
pub mod shadcn_dialog_requires_title;
pub mod shadcn_no_custom_badge;
pub mod shadcn_no_custom_skeleton;
pub mod shadcn_no_hr_use_separator;
pub mod shadcn_no_manual_dark_overrides;
pub mod shadcn_no_manual_zindex_overlays;
pub mod shadcn_no_raw_tailwind_colors;
pub mod shadcn_no_space_x_y;
pub mod shadcn_no_toggle_group_manual;
pub mod shadcn_sheet_requires_title;
pub mod shadcn_tabs_trigger_in_list;
pub mod sql_add_constraint_not_valid;
pub mod sql_boolean_column_prefix;
pub mod sql_constraint_naming_convention;
pub mod sql_fk_naming_convention;
pub mod sql_helpers;
pub mod sql_no_disable_autovacuum;
pub mod sql_no_drop_column_without_expand;
pub mod sql_no_function_on_indexed_column;
pub mod sql_no_is_deleted_boolean;
pub mod sql_no_now_in_transaction;
pub mod sql_no_rename_column;
pub mod sql_no_reserved_keyword_identifiers;
pub mod sql_no_select_then_insert_race;
pub mod sql_no_truncate_in_app;
pub mod sql_no_union_when_union_all;
pub mod sql_no_uuidv4_primary_key;
pub mod sql_require_search_path;
pub mod sql_singular_table_names;
pub mod tailwind_min_touch_target;
pub mod tailwind_no_legacy_directives;
pub mod tailwind_no_manual_dark_variants;
pub mod tailwind_no_off_scale_spacing;
pub mod tailwind_no_raw_color_utilities;
pub mod tailwind_no_tailwindcss_animate;
pub mod tailwind_no_transition_all_layout;
pub mod tailwind_require_focus_ring;
pub mod tailwind_require_motion_reduce;
pub mod tailwind_require_responsive_grid;
pub mod tailwind_require_responsive_text;
pub mod tanstack_query_dependent_needs_enabled;
pub mod tanstack_query_infinite_initial_page_param;
pub mod tanstack_query_invalidate_after_mutation;
pub mod tanstack_query_max_pages_requires_both;
pub mod tanstack_query_no_enabled_on_suspense;
pub mod tanstack_query_no_global_onerror_v5;
pub mod tanstack_query_no_v4_import_path;
pub mod tanstack_query_object_syntax;
pub mod tanstack_query_pass_signal_to_fetch;
pub mod tanstack_query_serializable_key;
pub mod tanstack_query_test_retry_false;
pub mod tanstack_start_api_route_json_helper;
pub mod tanstack_start_no_date_now_in_render;
pub mod tanstack_start_no_fetch_to_own_api;
pub mod tanstack_start_no_window_in_render;
pub mod tanstack_start_route_protection_beforeload;
pub mod tanstack_start_server_fn_post_for_mutations;
pub mod tanstack_start_server_fn_use_notfound;
pub mod tanstack_start_session_cookie_httponly;
pub mod tanstack_start_session_cookie_samesite;
pub mod tanstack_start_session_cookie_secure;
pub mod tanstack_start_session_secret_min_length;
pub mod testing_no_concurrent_without_context_expect;
pub mod testing_no_conditional_assertion;
pub mod testing_no_mocking_internal_modules;
pub mod testing_no_mocktimers_without_restore;
pub mod testing_no_shared_state;
pub mod testing_no_stubglobal_without_restore;
pub mod testing_no_try_catch_swallow;
pub mod testing_require_testid_kebab_case;
pub mod ts_assertion_fn_must_be_declaration;
pub mod ts_bounded_recursive_generic;
pub mod ts_branded_type_no_direct_cast;
pub mod ts_declare_global_requires_export;
pub mod ts_no_as_narrowing;
pub mod ts_no_generic_return_only;
pub mod ts_no_large_string_union;
pub mod ts_no_mixed_decorator_systems;
pub mod ts_no_mixed_sync_async_returns;
pub mod ts_no_narrowing_across_closures;
pub mod ts_no_unused_generic_parameter;
pub mod ts_overload_signature_order;
pub mod ts_prefer_interface_extends;
pub mod ts_require_variance_annotation;
pub mod ui_animate_presence_requires_exit;
pub mod ui_animate_transform_opacity_only;
pub mod ui_antialiased_on_root;
pub mod ui_concentric_border_radius;
pub mod ui_exit_duration_shorter_enter;
pub mod ui_hover_gated_media_query;
pub mod ui_min_hit_area_44;
pub mod ui_no_display_none_exit;
pub mod ui_no_keyframes_for_interruptible;
pub mod ui_no_pure_black;
pub mod ui_no_scroll_trigger_markers_prod;
pub mod ui_no_transition_all;
pub mod ui_prefers_reduced_motion;
pub mod ui_stagger_children_cap;
pub mod ui_symmetric_initial_exit;
pub mod ui_tabular_nums_on_data;
pub mod ui_text_balance_headings;
pub mod vue_computed_no_side_effects;
pub mod vue_custom_directive_v_prefix;
pub mod vue_define_model_over_modelvalue;
pub mod vue_inject_key_typed;
pub mod vue_no_filter_sort_in_template;
pub mod vue_no_ssr_globals_in_setup;
pub mod vue_no_usestore_top_level;
pub mod vue_no_v_if_with_v_for;
pub mod vue_no_value_on_reactive;
pub mod vue_no_watch_reactive_property;
pub mod vue_ref_value_in_script;
pub mod vue_scoped_styles_preferred;
pub mod vue_setup_store_return_all;
pub mod vue_sfc;
pub mod vue_shallowref_for_primitives;
pub mod vue_template_helpers;
pub mod vue_typed_define_props_emits;
pub mod vue_use_template_ref;
pub mod vue_v_memo_requires_v_for;
pub mod vue_watch_immediate_over_onmounted;
pub mod vue_withdefaults_factory;
pub mod vue_button_has_type;
pub mod vue_checked_requires_onchange;
pub mod vue_component_pascal_case;
pub mod vue_iframe_missing_sandbox;
pub mod vue_no_adjacent_inline_elements;
pub mod vue_no_array_index_key;
pub mod vue_no_comment_textnodes;
pub mod vue_no_duplicate_props;
pub mod vue_no_invalid_html_attribute;
pub mod vue_no_namespace;
pub mod vue_no_script_url;
pub mod vue_no_target_blank;
pub mod vue_no_unescaped_entities;
pub mod vue_props_no_spread_multi;
pub mod vue_self_closing_comp;
pub mod vue_void_elements_no_children;
pub mod yaml_k8s_helpers;
pub mod zod_no_coerce_on_financial;
pub mod zod_no_manual_types;
pub mod zod_no_schema_in_hot_path;
pub mod zod_prefer_extend_over_merge;
pub mod zod_prefer_loose_object;
pub mod zod_prefer_overwrite_v4;
pub mod zod_prefer_strict_object;
pub mod zod_prefer_stringbool;
pub mod zod_record_two_args;
pub mod zod_require_input_for_transforms;
pub mod zod_require_multipleof_currency;
// rust_must_use_on_result intentionally not declared — see mod.rs
// below for the rationale.
pub mod cognitive_complexity;
pub mod display_name;
pub mod generator_without_yield;
pub mod god_module;
pub mod halstead_complexity;
pub mod jsdoc_needs_description;
pub mod mysql_no_multiple_statements;
pub mod no_abbreviated_names;
pub mod no_alias_methods;
pub mod no_all_duplicated_branches;
pub mod no_and_in_function_name;
pub mod no_auth_token_in_localstorage;
pub mod no_boolean_flag_param;
pub mod no_clear_text_protocol;
pub mod no_collapsible_if;
pub mod no_commented_out_code;
pub mod no_common_grab_bag;
pub mod no_dangerously_set_inner_html;
pub mod no_done_callback;
pub mod no_double_cast;
pub mod no_duplicate_string;
pub mod no_enum;
pub mod no_equals_in_for_termination;
pub mod no_error_details_in_response;
pub mod no_eval;
pub mod no_extraneous_import;
pub mod no_fire_event;
pub mod no_focused_test;
pub mod no_for_in_iterable;
pub mod no_function_declaration_in_block;
pub mod no_function_overloads;
pub mod no_generic_names;
pub mod no_global_types_file;
pub mod no_gratuitous_expression;
pub mod no_hardcoded_ip;
pub mod no_hardcoded_secret;
pub mod no_identical_functions;
pub mod no_identical_title;
pub mod no_ignored_exceptions;
pub mod no_indexof_equality;
pub mod no_inline_function_event_listener;
pub mod no_inline_param_type;
pub mod no_interpolation_in_snapshots;
pub mod no_inverted_boolean_check;
pub mod no_json_parse_cast;
pub mod no_large_snapshots;
pub mod no_manual_rtl_cleanup;
pub mod no_mass_assignment;
pub mod no_match_snapshot;
pub mod no_misleading_collection_name;
pub mod no_mock_fetch_directly;
pub mod no_multi_op_oneliner;
pub mod no_nested_switch;
pub mod no_nested_template_literal;
pub mod no_nested_ternary;
pub mod no_new_regex_with_variable;
pub mod no_nullish_default_on_input;
pub mod no_one_iteration_loop;
pub mod no_open_redirect;
pub mod no_page_click_deprecated;
pub mod no_path_traversal;
pub mod no_property_mutation;
pub mod no_prototype_pollution;
pub mod no_redundant_assignment;
pub mod no_redundant_boolean;
pub mod no_section_divider_comments;
pub mod no_set_x_to_y;
pub mod no_shell_exec;
pub mod no_side_effects_in_initialization;
pub mod no_sort_without_comparator;
pub mod no_ssrf_fetch;
pub mod no_submit_handler_without_prevent_default;
pub mod no_sync_scripts;
pub mod no_test_logic;
pub mod no_test_prefixes;
pub mod no_test_return_statement;
pub mod no_throw;
pub mod no_type_assertion;
pub mod no_type_encoded_names;
pub mod no_unsanitized_property;
pub mod no_unsupported_node_builtins;
pub mod no_unvalidated_url_redirect;
pub mod no_valueof_field;
pub mod no_verb_in_rest_url;
pub mod no_wait_for_timeout;
pub mod no_while_loop;
pub mod object_literal;
pub mod operation_returning_nan;
pub mod os_command;
pub mod prefer_called_exactly_once_with;
pub mod prefer_called_with;
pub mod prefer_early_return;
pub mod prefer_expect_resolves;
pub mod prefer_exponentiation_operator;
pub mod prefer_immediate_return;
pub mod prefer_less_than;
pub mod prefer_mock_promise_shorthand;
pub mod prefer_object_has_own;
pub mod prefer_single_boolean_return;
pub mod prefer_spy_on;
pub mod prefer_switch_over_chained_if;
pub mod prefer_timer_args;
pub mod prefer_todo;
pub mod prefer_type_over_interface;
pub mod prefer_url_canparse;
pub mod react_duplicate_use_directive;
pub mod react_hoist_regex_outside_component;
pub mod react_hoist_static_jsx;
pub mod react_hook_form_destructuring_formstate;
pub mod react_layout_requires_children_prop;
pub mod react_no_and_conditional_jsx;
pub mod react_no_array_index_key;
pub mod react_no_async_client_component;
pub mod react_no_browser_api_in_server_component;
pub mod react_no_class_component_in_server_component;
pub mod react_no_client_hook_in_server_component;
pub mod react_no_client_only_in_server_component;
pub mod react_no_cookies_in_layout;
pub mod react_no_deprecated;
pub mod react_no_derived_state_in_effect;
pub mod react_no_empty_effect;
pub mod react_no_event_handler_in_server_component;
pub mod react_no_find_dom_node;
pub mod react_no_generate_static_params_in_client;
pub mod react_no_initialize_state_in_effect;
pub mod react_no_inline_default_prop;
pub mod react_no_metadata_export_in_client;
pub mod react_no_next_headers_in_client;
pub mod react_no_object_in_dep_array;
pub mod react_no_pass_data_to_parent;
pub mod react_no_render_return_value;
pub mod react_no_reset_all_state_on_prop_change;
pub mod react_no_sequential_await_in_component;
pub mod react_no_server_only_in_client;
pub mod react_passive_event_listeners;
pub mod react_prefer_react_cache;
pub mod react_prefer_use_transition;
pub mod react_server_action_requires_auth;
pub mod react_server_action_requires_validation;
pub mod react_use_state_initializer_function;
pub mod react_use_state_lazy_init;
pub mod regex_ast;
pub mod rust_anyhow_context_on_question_mark;
pub mod rust_arc_non_send_sync;
pub mod rust_await_holding_lock;
pub mod rust_block_on_in_async;
pub mod rust_builder_without_must_use;
pub mod rust_constants_top_of_file;
pub mod rust_duration_over_integer_with_unit;
pub mod rust_explicit_enum_match_arms;
pub mod rust_explicit_iter_loop;
pub mod rust_helpers;
pub mod rust_impl_debug_on_public_types;
pub mod rust_large_enum_variant;
pub mod rust_mod_tests_without_cfg_test;
pub mod rust_must_use_on_result_fn;
pub mod rust_no_as_numeric_cast;
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
pub mod rust_no_pub_use_glob;
pub mod rust_no_sleep_in_test;
pub mod rust_no_static_mut;
pub mod rust_no_todo_macro;
pub mod rust_no_unwrap;
pub mod rust_no_unwrap_in_from_impl;
pub mod rust_prefer_channel_over_arc_mutex_vec;
pub mod rust_prefer_once_lock;
pub mod rust_prefer_strum;
pub mod rust_prefer_tracing_over_log;
pub mod rust_prefer_unwrap_or_explicit;
pub mod rust_ptr_arg;
pub mod rust_pub_enum_without_non_exhaustive;
pub mod rust_rc_mutex;
pub mod rust_redundant_clone;
pub mod rust_serde_deny_unknown_fields;
pub mod rust_string_as_error;
pub mod rust_sync_io_in_async;
pub mod rust_thiserror_for_lib;
pub mod rust_thread_sleep_in_async;
pub mod rust_tokio_spawn_without_handle;
pub mod rust_unbounded_channel;
pub mod rust_undocumented_unsafe;
pub mod rust_unit_error_result;
pub mod rust_unsafe_ffi_isolation;
pub mod rust_unsafe_impl_without_comment;
pub mod rust_vec_with_capacity;
#[cfg(test)]
pub mod test_helpers;
pub mod test_methods;
// eslint-plugin-react rules (native implementations).
pub mod expression_complexity;
pub mod no_try_promise;
pub mod no_unused_collection;
pub mod prefer_while;
pub mod react_async_server_action;
pub mod react_button_has_type;
pub mod react_checked_requires_onchange;
pub mod react_forward_ref_uses_ref;
pub mod react_iframe_missing_sandbox;
pub mod react_jsx_key;
pub mod react_jsx_no_bind;
pub mod react_jsx_no_comment_textnodes;
pub mod react_jsx_no_duplicate_props;
pub mod react_jsx_no_jsx_as_prop;
pub mod react_jsx_no_new_array_as_prop;
pub mod react_jsx_no_new_object_as_prop;
pub mod react_jsx_no_script_url;
pub mod react_jsx_no_target_blank;
pub mod react_jsx_no_useless_fragment;
pub mod react_jsx_pascal_case;
pub mod react_jsx_props_no_spread_multi;
pub mod react_no_access_state_in_setstate;
pub mod react_no_adjacent_inline_elements;
pub mod react_no_chain_state_updates;
pub mod react_no_children_prop;
pub mod react_no_constructed_context_values;
pub mod react_no_danger_with_children;
pub mod react_no_invalid_html_attribute;
pub mod react_no_namespace;
pub mod react_no_object_type_as_default_prop;
pub mod react_no_string_refs;
pub mod react_no_this_in_sfc;
pub mod react_no_typos;
pub mod react_no_unescaped_entities;
pub mod react_no_unstable_nested_components;
pub mod react_self_closing_comp;
pub mod react_style_prop_object;
pub mod react_void_dom_elements_no_children;
pub mod sql_advisory_lock_prefer_xact;
pub mod sql_create_index_concurrently;
pub mod sql_index_needs_rationale_comment;
pub mod sql_no_between_timestamp;
pub mod sql_no_float_for_money;
pub mod sql_no_like_wildcard_prefix;
pub mod sql_no_offset_pagination;
pub mod sql_no_pg_enum;
pub mod sql_no_select_star;
pub mod sql_no_varchar;
pub mod sql_nullable_requires_comment;
pub mod sql_prefer_exists_over_in;
pub mod sql_require_transaction_timeout;
pub mod tailwind_classnames_order;
pub mod tailwind_enforces_negative_arbitrary_values;
pub mod tailwind_no_apply_for_variants;
pub mod tailwind_no_arbitrary_z_index;
pub mod tailwind_no_conflicting_classes;
pub mod tailwind_no_deprecated_classes;
pub mod tailwind_no_duplicate_classes;
pub mod tailwind_no_dynamic_class;
pub mod tailwind_no_important_modifier;
pub mod tailwind_no_unnecessary_whitespace;
pub mod tailwind_prefer_cn_utility;
pub mod tailwind_prefer_shorthand;
pub mod tailwind_prefer_size_shorthand;
pub mod tanstack_query_array_key;
pub mod tanstack_query_fn_must_throw_on_error;
pub mod tanstack_query_key_includes_params;
pub mod tanstack_query_no_cache_time;
pub mod tanstack_query_no_deprecated_props;
pub mod tanstack_query_no_enabled_true;
pub mod tanstack_query_no_is_loading;
pub mod tanstack_query_no_keep_previous_data_prop;
pub mod tanstack_query_no_query_callbacks;
pub mod tanstack_query_no_use_error_boundary;
pub mod tanstack_query_prefer_key_factory;
pub mod tanstack_query_prefer_query_options;
pub mod tanstack_query_prefer_suspense_query;
pub mod tanstack_query_require_stale_time;
pub mod tanstack_start_require_validate_search;
pub mod tanstack_start_server_fn_file_convention;
pub mod tanstack_start_server_fn_requires_auth;
pub mod tanstack_start_server_fn_requires_validation;
pub mod timeout_on_io;
pub mod vue_define_emits_typed;
pub mod vue_markraw_for_third_party;
pub mod vue_no_duplicate_v_if;
pub mod vue_no_options_api;
pub mod vue_no_reactive_destructure;
pub mod vue_no_v_html_unsafe;
pub mod vue_pinia_store_to_refs;
pub mod vue_prefer_computed;
pub mod vue_prefer_v_else;
pub mod vue_require_lifecycle_cleanup;
pub mod vue_script_setup_required;
pub mod vue_sfc_section_order;
pub mod vue_url_state_for_filters;
pub mod vue_v_for_needs_stable_key;
pub mod walker;
pub mod xstate_spawn_usage;
pub mod zod_consistent_import_source;
pub mod zod_no_any;
pub mod zod_no_empty_custom_schema;
pub mod zod_no_number_schema_with_int;
pub mod zod_no_optional_nullable_chain;
pub mod zod_no_string_schema_with_uuid;
pub mod zod_no_throw_in_refine;
pub mod zod_no_transform_in_record_key;
pub mod zod_prefer_discriminated_union;
pub mod zod_prefer_enum_over_literal_union;
pub mod zod_prefer_safe_parse;
pub mod zod_prefer_top_level_format;
pub mod zod_refine_requires_path;
pub mod zod_require_error_messages;
pub mod zod_string_min_1_required;
pub mod zod_trim_before_min;

pub mod a11y_alt_text;
pub mod a11y_anchor_ambiguous_text;
pub mod a11y_anchor_has_content;
pub mod a11y_anchor_is_valid;
pub mod a11y_aria_activedescendant_has_tabindex;
pub mod a11y_aria_props;
pub mod a11y_aria_role;
pub mod a11y_aria_unsupported_elements;
pub mod a11y_autocomplete_valid;
pub mod a11y_click_events_have_key_events;
pub mod a11y_control_has_associated_label;
pub mod a11y_heading_has_content;
pub mod a11y_html_has_lang;
pub mod a11y_iframe_has_title;
pub mod a11y_img_redundant_alt;
pub mod a11y_interactive_supports_focus;
pub mod a11y_label_has_associated_control;
pub mod a11y_media_has_caption;
pub mod a11y_mouse_events_have_key_events;
pub mod a11y_no_access_key;
pub mod a11y_no_aria_hidden_on_focusable;
pub mod a11y_no_autofocus;
pub mod a11y_no_distracting_elements;
pub mod a11y_no_interactive_element_to_noninteractive_role;
pub mod a11y_no_noninteractive_element_interactions;
pub mod a11y_no_noninteractive_element_to_interactive_role;
pub mod a11y_no_noninteractive_tabindex;
pub mod a11y_no_redundant_roles;
pub mod a11y_no_static_element_interactions;
pub mod a11y_prefer_tag_over_role;
pub mod a11y_role_has_required_aria_props;
pub mod a11y_scope;
pub mod a11y_tabindex_no_positive;
pub mod api_deprecation_headers;
pub mod api_first;
pub mod api_import_from_public_index;
pub mod api_list_requires_pagination;
pub mod api_no_array_root_response;
pub mod api_no_boolean_field_in_response;
pub mod arguments_order;
pub mod array_callback_without_return;
pub mod assertions_in_tests;
pub mod audit_log_required_fields;
pub mod auth_on_mutation;
pub mod comma_or_logical_or_case;
pub mod cyclomatic_complexity;
pub mod data_clumps;
pub mod dead_export;
pub mod duplicate_export;
pub mod elseif_without_else;
pub mod error_message_is_remediation;
pub mod factory_di_shape;
pub mod file_extension_in_import;
pub mod filename_naming_convention;
pub mod for_loop_increment_sign;
pub mod function_component_definition;
pub mod function_inside_loop;
pub mod function_return_type;
pub mod hono_cookie_no_httponly;
pub mod hono_cookie_no_samesite;
pub mod hono_cookie_no_secure;
pub mod hono_cors_permissive;
pub mod hono_csp_unsafe;
pub mod hono_csrf_missing;
pub mod hono_jwt_secret_hardcoded;
pub mod hono_missing_secure_headers;
pub mod hono_no_get_with_body;
pub mod hono_no_hardcoded_cors_origin;
pub mod hono_secure_headers_disabled;
pub mod hook_use_state;
pub mod html_no_abstract_roles;
pub mod html_no_aria_hidden_body;
pub mod html_no_duplicate_attrs;
pub mod html_no_duplicate_id;
pub mod html_no_nested_interactive;
pub mod html_no_non_scalable_viewport;
pub mod html_no_obsolete_tags;
pub mod html_no_positive_tabindex;
pub mod html_no_script_style_type;
pub mod html_no_skip_heading_levels;
pub mod html_prefer_https;
pub mod html_require_button_type;
pub mod html_require_closing_tags;
pub mod html_require_doctype;
pub mod html_require_explicit_size;
pub mod html_require_img_alt;
pub mod html_require_input_label;
pub mod html_require_meta_charset;
pub mod html_require_title;
pub mod inconsistent_function_call;
pub mod index_of_compare_to_positive;
pub mod intermediate_variables;
pub mod inverted_assertion_arguments;
pub mod jsdoc_informative_docs;
pub mod jsdoc_reject_any_type;
pub mod jsdoc_reject_function_type;
pub mod jsx_ensure_booleans;
pub mod jsx_filename_extension;
pub mod jsx_fragments;
pub mod jsx_handler_names;
pub mod jsx_no_leaked_render;
pub mod jsx_no_new_function_as_prop;
pub mod jsx_no_undef;
pub mod justify_inaction;
pub mod max_call_chain_depth;
pub mod max_union_size;
pub mod nested_control_flow;
pub mod no_arguments_usage;
pub mod no_array_constructor;
pub mod no_array_delete;
pub mod no_associative_arrays;
pub mod no_async_array_callback;
pub mod no_async_constructor;
pub mod no_async_without_await;
pub mod no_bidi_characters;
pub mod no_bitwise_in_boolean;
pub mod no_built_in_override;
pub mod no_case_label_in_switch;
pub mod no_class_inheritance;
pub mod no_collection_size_mischeck;
pub mod no_confidential_logging;
pub mod no_constructor_side_effects;
pub mod no_deprecated_api;
pub mod no_deprecated_cipher;
pub mod no_disable_mustache_escape;
pub mod no_duplicate_in_composite;
pub mod no_duplicated_branches;
pub mod no_dynamic_template;
pub mod no_ecb_mode;
pub mod no_electron_node_integration;
pub mod no_element_overwrite;
pub mod no_empty_test_file;
pub mod no_floating_promise;
pub mod no_globals_shadowing;
pub mod no_hardcoded_secret_signature;
pub mod no_hook_setter_in_body;
pub mod no_identical_conditions;
pub mod no_identical_expressions;
pub mod no_ignored_return;
pub mod no_implicit_deps;
pub mod no_in_misuse;
pub mod no_incomplete_assertions;
pub mod no_inconsistent_returns;
pub mod no_incorrect_string_concat;
pub mod no_inferred_any;
pub mod no_insecure_jwt;
pub mod no_invariant_returns;
pub mod no_logger_in_business_logic;
pub mod no_loop_counter_reassign;
pub mod no_misleading_array_reverse;
pub mod no_misplaced_loop_counter;
pub mod no_nested_assignment;
pub mod no_nested_functions;
pub mod no_nested_incdec;
pub mod no_post_message_star;
pub mod no_primitive_wrappers;
pub mod no_promise_reject;
pub mod no_raw_db_entity_in_handler;
pub mod no_redundant_await;
pub mod no_redundant_clsx;
pub mod no_redundant_jump;
pub mod no_redundant_optional;
pub mod no_redundant_state;
pub mod no_return_type_any;
pub mod no_same_argument_assert;
pub mod no_small_switch;
pub mod no_timing_attack;
pub mod no_try_statements;
pub mod no_undefined_argument;
pub mod no_undefined_assignment;
pub mod no_unenclosed_multiline_block;
pub mod no_uniq_key;
pub mod no_unthrown_error;
pub mod no_unused_locators;
pub mod no_unverified_certificate;
pub mod no_unverified_hostname;
pub mod no_useless_increment;
pub mod no_useless_intersection;
pub mod no_useless_react_setstate;
pub mod no_weak_cipher;
pub mod no_weak_hashing;
pub mod no_weak_keys;
pub mod no_weak_ssl;
pub mod no_xml_external_entity;
pub mod non_existent_operator;
pub mod option_vs_result;
pub mod playwright_no_duplicate_slow;
pub mod playwright_no_slowed_test;
pub mod prefer_default_last;
pub mod prefer_destructuring_assignment;
pub mod prefer_object_literal;
pub mod prefer_promise_shorthand;
pub mod prefer_regexp_exec;
pub mod prefer_type_guard;
pub mod public_static_readonly;
pub mod pure_by_default;
pub mod redundant_type_aliases;
pub mod regex_anchor_precedence;
pub mod regex_complexity;
pub mod regex_confusing_quantifier;
pub mod regex_no_contradiction_with_assertion;
pub mod regex_no_control_chars;
pub mod regex_no_dupe_disjunctions;
pub mod regex_no_duplicate_chars;
pub mod regex_no_empty_after_reluctant;
pub mod regex_no_empty_alternative;
pub mod regex_no_empty_character_class;
pub mod regex_no_empty_group;
pub mod regex_no_empty_lookaround;
pub mod regex_no_empty_string_literal_v;
pub mod regex_no_empty_string_match;
pub mod regex_no_escape_backspace;
pub mod regex_no_extra_lookaround_assertions;
pub mod regex_no_invisible_character;
pub mod regex_no_legacy_features;
pub mod regex_no_misleading_capturing_group;
pub mod regex_no_misleading_char_class;
pub mod regex_no_missing_g_flag;
pub mod regex_no_multiple_spaces;
pub mod regex_no_non_standard_flag;
pub mod regex_no_obscure_range;
pub mod regex_no_octal;
pub mod regex_no_optional_assertion;
pub mod regex_no_potentially_useless_backreference;
pub mod regex_no_single_char_class;
pub mod regex_no_slow_pattern;
pub mod regex_no_standalone_backslash;
pub mod regex_no_stateful_global;
pub mod regex_no_super_linear_move;
pub mod regex_no_trivially_nested_assertion;
pub mod regex_no_trivially_nested_quantifier;
pub mod regex_no_unused_groups;
pub mod regex_no_useless_assertions;
pub mod regex_no_useless_backreference;
pub mod regex_no_useless_dollar_replacements;
pub mod regex_no_useless_flag;
pub mod regex_no_useless_lazy;
pub mod regex_no_useless_quantifier;
pub mod regex_no_useless_set_operand;
pub mod regex_no_useless_string_literal;
pub mod regex_no_useless_two_nums_quantifier;
pub mod regex_no_zero_quantifier;
pub mod regex_optimal_lookaround_quantifier;
pub mod regex_prefer_char_class;
pub mod regex_prefer_predefined_assertion;
pub mod regex_prefer_quantifier;
pub mod regex_prefer_set_operation;
pub mod regex_sort_flags;
pub mod regex_use_unicode_flag;
pub mod strings_comparison;
pub mod structured_api_error;
pub mod symmetric_pairs;
pub mod test_check_exception;
pub mod testing_no_and_in_test_name;
pub mod testing_no_undefined_mock_var;
pub mod testing_prefer_msw;
pub mod testing_prefer_test_each;
pub mod too_many_break_or_continue;
pub mod unused_component_prop;
pub mod unused_enum_member;
pub mod use_type_alias;
pub mod useless_string_operation;
pub mod valid_expect;
pub mod valid_expect_in_promise;

// eslint-plugin-import rules (native implementations).
pub mod exports_last;
pub mod id_length;
pub mod import_consistent_type_specifier_style;
pub mod import_default;
pub mod import_dynamic_import_chunkname;
pub mod import_export;
pub mod import_named;
pub mod import_namespace;
pub mod import_no_amd;
pub mod import_no_commonjs;
pub mod import_no_cycle;
pub mod type_only_dependency;
pub mod unlisted_dependency;
pub mod unused_dependency;
pub mod unused_file;
pub mod import_no_duplicates;
pub mod import_no_dynamic_require;
pub mod import_no_empty_named_blocks;
pub mod import_no_named_as_default;
pub mod import_no_unresolved;
pub mod import_no_webpack_loader_syntax;
pub mod imports_first;
pub mod max_dependencies;
pub mod newline_after_import;
pub mod no_absolute_path;
pub mod no_duplicate_imports;
pub mod no_import_dist;
pub mod no_import_module_exports;
pub mod no_import_node_modules_by_path;
pub mod no_import_node_test;
pub mod no_mocks_import;
pub mod no_mutable_exports;
pub mod no_self_import;
pub mod no_unassigned_import;
pub mod no_useless_path_segments;
pub mod require_not_empty;

// eslint-plugin-unicorn rules (native implementations).
pub mod catch_error_name;
pub mod consistent_date_clone;
pub mod consistent_destructuring;
pub mod consistent_empty_array_spread;
pub mod consistent_existence_index_check;
pub mod consistent_function_scoping;
pub mod consistent_template_literal_escape;
pub mod custom_error_definition;
pub mod detect_dangerous_redirects;
pub mod detect_option_rejectunauthorized;
pub mod empty_brace_spaces;
pub mod error_message;
pub mod escape_case;
pub mod expiring_todo_comments;
pub mod explicit_length_check;
pub mod nestjs_controller_return_type;
pub mod nestjs_no_entity_in_controller;
pub mod nestjs_no_forwardref_abuse;
pub mod new_for_builtins;
pub mod next_no_redirect_in_try_catch;
pub mod no_abusive_eslint_disable;
pub mod no_accessor_recursion;
pub mod no_anonymous_default_export;
pub mod no_array_callback_reference;
pub mod no_array_method_this_argument;
pub mod no_array_reduce;
pub mod no_array_reverse;
pub mod no_array_sort_mutation;
pub mod no_assign_mutated_array;
pub mod no_await_expression_member;
pub mod no_await_in_promise_methods;
pub mod no_catch_log_rethrow;
pub mod no_catch_without_use;
pub mod no_console_spaces;
pub mod no_delete;
pub mod no_document_cookie;
pub mod no_document_domain;
pub mod no_document_write;
pub mod no_empty_catch;
pub mod no_empty_file;
pub mod no_extra_arguments;
pub mod no_for_loop;
pub mod no_hex_escape;
pub mod no_immediate_mutation;
pub mod no_inner_html;
pub mod no_instanceof_builtins;
pub mod no_invalid_fetch_options;
pub mod no_invalid_remove_event_listener;
pub mod no_keyword_prefix;
pub mod no_let;
pub mod no_lonely_if;
pub mod no_magic_array_flat_depth;
pub mod no_mutating_assign;
pub mod no_mutating_methods;
pub mod no_mutation;
pub mod no_named_default;
pub mod no_negated_condition;
pub mod no_negation_in_equality_check;
pub mod no_null;
pub mod no_object_as_default_parameter;
pub mod no_process_exit;
pub mod no_single_promise_in_promise_methods;
pub mod no_static_only_class;
pub mod no_thenable;
pub mod no_this_assignment;
pub mod no_this_mutation;
pub mod no_typeof_undefined;
pub mod no_unknown_property;
pub mod no_unnecessary_array_flat_depth;
pub mod no_unnecessary_array_splice_count;
pub mod no_unnecessary_await;
pub mod no_unnecessary_slice_end;
pub mod no_unreadable_array_destructuring;
pub mod no_unreadable_iife;
pub mod no_unsafe_alloc;
pub mod no_unsafe_shell_exec;
pub mod no_useless_collection_argument;
pub mod no_useless_error_capture_stack_trace;
pub mod no_useless_fallback_in_spread;
pub mod no_useless_iterator_to_array;
pub mod no_useless_length_check;
pub mod no_useless_promise_resolve_reject;
pub mod no_useless_spread;
pub mod no_useless_switch_case;
pub mod no_zero_fractions;
pub mod node_callback_return;
pub mod node_global_require;
pub mod node_handle_callback_err;
pub mod node_hashbang;
pub mod node_no_callback_literal;
pub mod node_no_exports_assign;
pub mod node_no_mixed_requires;
pub mod node_no_new_require;
pub mod node_no_path_concat;
pub mod node_no_process_env;
pub mod node_no_sync;
pub mod node_no_top_level_await;
pub mod node_no_unhandled_rejection;
pub mod node_prefer_promises_dns;
pub mod node_prefer_promises_fs;
pub mod node_prefer_stream_pipeline;
pub mod number_literal_case;
pub mod numeric_separators_style;
pub mod prefer_add_event_listener;
pub mod prefer_array_fill;
pub mod prefer_array_find;
pub mod prefer_array_flat;
pub mod prefer_array_from_map;
pub mod prefer_array_index_of;
pub mod prefer_array_some;
pub mod prefer_array_to_reversed;
pub mod prefer_array_to_sorted;
pub mod prefer_array_to_spliced;
pub mod prefer_at;
pub mod prefer_bigint_literals;
pub mod prefer_blob_reading_methods;
pub mod prefer_class_fields;
pub mod prefer_classlist_toggle;
pub mod prefer_code_point;
pub mod prefer_date_now;
pub mod prefer_default_parameters;
pub mod prefer_dom_node_append;
pub mod prefer_dom_node_dataset;
pub mod prefer_dom_node_remove;
pub mod prefer_dom_node_text_content;
pub mod prefer_event_target;
pub mod prefer_export_from;
pub mod prefer_global_this;
pub mod prefer_import_meta_properties;
pub mod prefer_includes;
pub mod prefer_json_parse_buffer;
pub mod prefer_keyboard_event_key;
pub mod prefer_lazy_load;
pub mod prefer_logical_operator_over_ternary;
pub mod prefer_math_min_max;
pub mod prefer_math_trunc;
pub mod prefer_mock_return_shorthand;
pub mod prefer_modern_dom_apis;
pub mod prefer_modern_math_apis;
pub mod prefer_module;
pub mod prefer_native_coercion_functions;
pub mod prefer_negative_index;
pub mod prefer_node_protocol;
pub mod prefer_number_properties;
pub mod prefer_object_from_entries;
pub mod prefer_optional_catch_binding;
pub mod prefer_prototype_methods;
pub mod prefer_query_selector;
pub mod prefer_reflect_apply;
pub mod prefer_regexp_test;
pub mod prefer_response_static_json;
pub mod prefer_set_has;
pub mod prefer_set_size;
pub mod prefer_single_call;
pub mod prefer_spread;
pub mod prefer_static_regex;
pub mod prefer_string_raw;
pub mod prefer_string_replace_all;
pub mod prefer_string_slice;
pub mod prefer_string_starts_ends_with;
pub mod prefer_string_trim_start_end;
pub mod prefer_structured_clone;
pub mod prefer_ternary;
pub mod prefer_to_have_length;
pub mod prefer_top_level_await;
pub mod prefer_type_error;
pub mod react_no_javascript_urls;
pub mod relative_url_style;
pub mod require_array_join_separator;
pub mod require_explicit_undefined;
pub mod require_hook;
pub mod require_module_attributes;
pub mod require_module_specifiers;
pub mod require_number_to_fixed_digits_argument;
pub mod require_path_exists;
pub mod require_post_message_target_origin;
pub mod require_to_throw_message;
pub mod require_too_many_arguments;
pub mod switch_case_braces;
pub mod switch_case_break_position;
pub mod template_indent;
pub mod text_encoding_identifier_case;
pub mod throw_error_values;
pub mod throw_new_error;
pub mod try_catch_json_parse;
pub mod try_catch_new_url;
// typescript-eslint rules (native implementations).
pub mod ts_adjacent_overload_signatures;
pub mod ts_ban_ts_comment;
pub mod ts_ban_tslint_comment;
pub mod ts_class_literal_property_style;
pub mod ts_class_methods_use_this;
pub mod ts_consistent_generic_constructors;
pub mod ts_consistent_indexed_object_style;
pub mod ts_consistent_type_assertions;
pub mod ts_consistent_type_exports;
pub mod ts_consistent_type_imports;
pub mod ts_default_param_last;
pub mod ts_explicit_function_return_type;
pub mod ts_explicit_member_accessibility;
pub mod ts_explicit_module_boundary_types;
pub mod ts_init_declarations;
pub mod ts_max_params;
pub mod ts_member_ordering;
pub mod ts_method_signature_style;
pub mod ts_no_array_constructor;
pub mod ts_no_confusing_non_null_assertion;
pub mod ts_no_const_enum;
pub mod ts_no_dupe_class_members;
pub mod ts_no_duplicate_enum_values;
pub mod ts_no_dynamic_delete;
pub mod ts_no_empty_function;
pub mod ts_no_empty_object_type;
pub mod ts_no_export_equal;
pub mod ts_no_extra_non_null_assertion;
pub mod ts_no_extraneous_class;
pub mod ts_no_implicit_any_catch;
pub mod ts_no_import_type_side_effects;
pub mod ts_no_inferrable_types;
pub mod ts_no_invalid_this;
pub mod ts_no_invalid_void_type;
pub mod ts_no_loop_func;
pub mod ts_no_magic_numbers;
pub mod ts_no_misused_new;
pub mod ts_no_mixed_types;
pub mod ts_no_namespace;
pub mod ts_no_non_null_asserted_nullish_coalescing;
pub mod ts_no_non_null_asserted_optional_chain;
pub mod ts_no_non_null_assertion;
pub mod ts_no_redeclare;
pub mod ts_no_restricted_imports;
pub mod ts_no_restricted_types;
pub mod ts_no_shadow;
pub mod ts_no_this_alias;
pub mod ts_no_unnecessary_parameter_property_assignment;
pub mod ts_no_unnecessary_type_constraint;
pub mod ts_no_unsafe_declaration_merging;
pub mod ts_no_unused_expressions;
pub mod ts_no_unused_private_class_members;
pub mod ts_no_unused_vars;
pub mod ts_no_use_before_define;
pub mod ts_no_useless_constructor;
pub mod ts_no_useless_empty_export;
pub mod ts_no_wrapper_object_types;
pub mod ts_only_throw_error;
pub mod ts_parameter_properties;
pub mod ts_prefer_for_of;
pub mod ts_prefer_function_type;
pub mod ts_prefer_literal_enum_member;
pub mod ts_prefer_promise_reject_errors;
pub mod ts_triple_slash_reference;
pub mod ts_unified_signatures;
// eslint-plugin-playwright rules (native implementations).
pub mod playwright_expect_expect;
pub mod playwright_max_expects;
pub mod playwright_max_nested_describe;
pub mod playwright_no_commented_out_tests;
pub mod playwright_no_conditional_expect;
pub mod playwright_no_conditional_in_test;
pub mod playwright_no_duplicate_hooks;
pub mod playwright_no_element_handle;
pub mod playwright_no_force_option;
pub mod playwright_no_hooks;
pub mod playwright_no_nested_step;
pub mod playwright_no_networkidle;
pub mod playwright_no_nth_methods;
pub mod playwright_no_page_pause;
pub mod playwright_no_raw_locators;
pub mod playwright_no_skipped_test;
pub mod playwright_no_standalone_expect;
pub mod playwright_no_unsafe_references;
pub mod playwright_no_useless_await;
pub mod playwright_no_useless_not;
pub mod playwright_no_wait_for_navigation;
pub mod playwright_no_wait_for_selector;
pub mod playwright_no_wait_for_timeout;
pub mod playwright_prefer_comparison_matcher;
pub mod playwright_prefer_equality_matcher;
pub mod playwright_prefer_hooks_in_order;
pub mod playwright_prefer_hooks_on_top;
pub mod playwright_prefer_native_locators;
pub mod playwright_prefer_strict_equal;
pub mod playwright_prefer_to_be;
pub mod playwright_prefer_to_contain;
pub mod playwright_prefer_to_have_count;
pub mod playwright_prefer_web_first_assertions;
// eslint-plugin-jsdoc rules (native implementations).
pub mod jsdoc_complete_sentence;
// eslint-plugin-de-morgan (native implementation).
pub mod de_morgan_simplify;
// eslint-plugin-react-refresh (native implementation).
pub mod react_refresh_only_export_components;
// eslint-plugin-playwright (native implementation).
pub mod comment_prose_quality;
pub mod layer_import_boundary;
pub mod no_index_file;
pub mod package_json_sorted_deps;
pub mod package_json_unique_deps;
pub mod playwright_missing_await;
pub mod playwright_no_eval;
pub mod top_level_function;
pub mod vitest_hoisted_apis_on_top;
pub mod vitest_no_disabled_tests;
// v3.0 — Skill-driven rules: Batch 1 (TypeScript/Architecture)
pub mod avoid_barrel_files;
pub mod avoid_importing_barrel_files;
pub mod avoid_re_export_all;
pub mod import_dedupe;
pub mod no_default_export;
pub mod no_full_import;
pub mod no_test_imports_in_prod;
pub mod prefer_promise_all;
pub mod ts_prefer_using_declaration;
// v3.0 — Skill-driven rules: Batch 11 (i18n)
pub mod better_auth_middleware_requires_headers;
pub mod better_auth_require_secure_cookies;
pub mod drizzle_no_push_in_production;
pub mod express_session_require_name;
pub mod i18n_json_identical_keys;
pub mod i18n_json_identical_placeholders;
pub mod i18n_json_no_empty_values;
pub mod i18n_json_no_nesting;
pub mod i18n_json_no_untranslated;
pub mod i18n_json_valid_message_syntax;
pub mod i18n_no_concat_translation_key;
pub mod i18n_no_hardcoded_string_in_jsx;
pub mod i18n_no_manual_pluralization;
pub mod i18n_no_string_concat_with_translation;
pub mod i18n_no_unnecessary_trans_component;
pub mod i18n_prefer_intl_api;
pub mod i18n_prefer_logical_css_properties;
pub mod no_conditional_async_return;
pub mod no_conditional_tests;
pub mod no_unchecked_json_parse;
pub mod no_unsanitized_method;
pub mod rust_no_mutex_in_single_threaded;
pub mod rust_prefer_cow;
pub mod rust_prefer_fast_hasher;
pub mod serialize_javascript_no_unsafe;
pub mod tailwind_no_magic_spacing;
pub mod tailwind_read_theme_before_classes;
pub mod tanstack_start_loader_stale_time;
pub mod tanstack_start_no_client_import_in_server_fn;
pub mod testing_no_real_external_service;
pub mod ts_prefer_satisfies;
pub mod valid_describe_callback;
pub mod vue_no_mutate_prop;
pub mod xpath_injection;
pub mod xstate_entry_exit_action;
pub mod xstate_event_names;
pub mod xstate_invoke_usage;
pub mod xstate_no_async_guard;
pub mod xstate_no_imperative_action;
pub mod xstate_no_infinite_loop;
pub mod xstate_no_inline_implementation;
pub mod xstate_no_invalid_conditional_action;
pub mod xstate_no_invalid_state_props;
pub mod xstate_no_invalid_transition_props;
pub mod xstate_no_misplaced_on_transition;
pub mod xstate_no_ondone_outside_compound_state;
pub mod xstate_state_names;
pub mod zod_brand_ids;
pub mod zod_no_optional_and_default_together;
pub mod zod_no_unknown_schema;
pub mod zod_require_schema_suffix;
pub mod zod_transform_requires_pipe;
pub mod zod_validate_env_at_startup;
use crate::diagnostic::Severity;
use crate::files::Language;
use backend::Backend;
use meta::RuleMeta;

/// A rule: identity + per-language enforcement backends.
#[derive(Debug)]
pub struct RuleDef {
    pub meta: RuleMeta,
    pub backends: Vec<(Language, Backend)>,
}

// Registry helpers + macros — moved to `registry.rs` and re-exported below.
mod registry;
pub use registry::{RustBinding, build_rust_only_rule, build_ts_family_rule};

pub mod meta_registry;
pub mod a11y_button_without_accessible_name;
pub mod a11y_dialog_missing_aria_labelledby;
pub mod api_no_status_in_body;
pub mod api_no_unbounded_input_field;
pub mod api_response_envelope_consistency;
pub mod drizzle_decimal_for_money;
pub mod drizzle_dollar_type_widens_unknown;
pub mod drizzle_findfirst_without_where;
pub mod drizzle_leftjoin_nullable_handling;
pub mod drizzle_migrations_no_data_in_schema_migration;
pub mod drizzle_no_db_query_in_loop;
pub mod drizzle_no_drizzle_kit_in_runtime;
pub mod drizzle_prefer_select_columns;
pub mod drizzle_relations_missing_inverse;
pub mod elysia_aot_dynamic_route;
pub mod elysia_decorate_uses_request_data;
pub mod elysia_derive_async_no_await;
pub mod elysia_guard_overrides_route_schema;
pub mod elysia_model_reference_by_string;
pub mod elysia_onerror_before_plugin;
pub mod elysia_response_t_unknown;
pub mod elysia_set_status_after_return;
pub mod elysia_t_unknown_format_string;
pub mod elysia_ws_message_no_schema;
pub mod next_no_server_action_without_use_server;
pub mod react_no_async_useeffect_callback;
pub mod react_no_form_action_async_no_pending;
pub mod react_no_ref_read_during_render;
pub mod react_no_router_push_in_effect_no_dep;
pub mod react_no_setstate_no_cancel_flag;
pub mod react_no_state_setter_in_render;
pub mod react_no_sync_layout_effect_in_ssr;
pub mod react_no_useeffect_event_in_deps;
pub mod react_prefer_use_action_state;
pub mod react_prefer_use_optimistic;
pub mod react_use_no_conditional;
pub mod rust_assert_eq_with_bool_literal;
pub mod rust_box_dyn_error_without_send_sync;
pub mod rust_clone_in_iter_chain;
pub mod rust_collect_then_into_iter;
pub mod rust_const_for_static_no_interior_mut;
pub mod rust_drop_calls_self_lock;
pub mod rust_env_var_unwrap_at_runtime;
pub mod rust_eprintln_in_library;
pub mod rust_float_eq_partial_cmp;
pub mod rust_format_args_in_log_macro;
pub mod rust_hash_partial_eq_mismatch;
pub mod rust_if_let_mutex_lock;
pub mod rust_iter_count_vs_len;
pub mod rust_loop_collect_into_existing_vec;
pub mod rust_match_lock_guard_scrutinee;
pub mod rust_ord_partial_ord_inconsistent;
pub mod rust_panic_in_drop;
pub mod rust_partial_eq_without_eq;
pub mod rust_repeated_string_concat_in_loop;
pub mod rust_select_without_biased;
pub mod rust_send_sync_unsafe_impl_on_pointer_field;
pub mod rust_serde_untagged_without_explicit_default;
pub mod rust_string_push_str_format;
pub mod rust_thread_spawn_in_async;
pub mod rust_to_string_in_format_arg;
pub mod sql_add_not_null_without_default;
pub mod sql_alter_column_type_unsafe;
pub mod sql_check_constraint_no_volatile_function;
pub mod sql_create_index_in_transaction;
pub mod sql_drop_table_no_cascade_warning;
pub mod sql_index_on_low_cardinality_boolean;
pub mod sql_jsonb_not_json;
pub mod sql_no_for_update_without_skip_locked;
pub mod sql_no_serial_use_identity;
pub mod sql_pg_enum_with_alter_type_add_value;
pub mod sql_recursive_cte_no_termination;
pub mod sql_text_search_missing_language;
pub mod tailwind_no_negative_z_index_on_interactive;
pub mod tailwind_no_overflow_hidden_on_focus_container;
pub mod tailwind_no_text_size_below_12px;
pub mod tailwind_no_w_screen_h_screen_on_mobile;
pub mod tailwind_require_aspect_ratio_on_media;
pub mod tanstack_query_dehydrate_no_pending_in_ssr;
pub mod tanstack_query_no_async_query_fn_without_await;
pub mod tanstack_query_no_query_in_render_loop;
pub mod tanstack_query_no_set_query_data_without_key_factory;
pub mod tanstack_query_select_must_be_stable;
pub mod tanstack_router_search_no_use_state_for_url_state;
pub mod ts_no_assert_never_in_default;
pub mod ts_no_enum_object_literal_pattern;
pub mod ts_no_floating_promise_in_array_method;
pub mod ts_no_object_keys_typed_loop;
pub mod ts_no_promise_void_function_misuse;
pub mod ts_no_redundant_async;
pub mod ts_no_void_returning_assigned;
pub mod ts_prefer_promise_with_resolvers;
pub mod zod_no_parse_in_render;
pub mod zod_no_passthrough_on_api_input;
pub mod zod_no_safeparse_without_check;

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
    // Dedupe by lint name — a clippy lint may be referenced by more
    // than one comply rule; keep the first binding so the clippy
    // scanner emits a single diagnostic per lint.
    bindings.sort_by_key(|(lint, _, _)| *lint);
    bindings.dedup_by_key(|(lint, _, _)| *lint);
    bindings
}

/// Accessor for tsgolint-delegated backends (type-aware rules).
/// Only used when --with-types is passed.
pub fn collect_tsgolint_bindings() -> Vec<(&'static str, &'static RuleMeta, Severity)> {
    let mut bindings = Vec::new();
    for rule in delegated::register_tsgolint() {
        let meta_static: &'static RuleMeta = Box::leak(Box::new(rule.meta));
        for (_lang, backend) in &rule.backends {
            if let Backend::Tsgolint { rule: tsgolint_key } = backend {
                bindings.push((*tsgolint_key, meta_static, meta_static.severity));
            }
        }
    }
    bindings.sort_by_key(|(key, _, _)| *key);
    bindings.dedup_by_key(|(key, _, _)| *key);
    bindings
}

/// All registered rules — both the custom ones and the oxlint-delegated ones.
pub fn all_rule_defs() -> Vec<RuleDef> {
    let mut rules = vec![
        no_throw::register(),
        no_async_without_await::register(),
        no_async_array_callback::register(),
        no_floating_promise::register(),
        no_redundant_await::register(),
        no_redundant_state::register(),
        no_unused_locators::register(),
        unused_component_prop::register(),
        unused_enum_member::register(),
        hook_use_state::register(),
        valid_expect::register(),
        valid_expect_in_promise::register(),
        playwright_no_duplicate_slow::register(),
        playwright_no_slowed_test::register(),
        function_component_definition::register(),
        jsx_filename_extension::register(),
        react_no_deprecated::register(),
        throw_error_values::register(),
        no_catch_without_use::register(),
        try_catch_json_parse::register(),
        try_catch_new_url::register(),
        no_catch_log_rethrow::register(),
        exception_use_error_cause::register(),
        no_nested_ternary::register(),
        // @TODO: il a flag le commentaire suivant :
        // // const foo =, let foo =, var foo =
        no_commented_out_code::register(),
        no_common_grab_bag::register(),
        boolean_naming::register(),
        boundary_condition::register(),
        mysql_no_multiple_statements::register(),
        no_boolean_flag_param::register(),
        explicit_units::register(),
        no_abbreviated_names::register(),
        no_generic_names::register(),
        // @TODO: flagged:
        // src/rules/comment_paraphrases_code/text.rs:31:17: warning [no-type-encoded-names] 'fn_name' encodes a type prefix 'fn' — Hungarian notation is obsolete. Remove the prefix; the type system already tells you the type.
        // let fn_name = extract_fn_name(trimmed);
        // -> ne pas faire les fonctions ? le faire que si le type est vraiment le meme que le nom ?
        no_type_assertion::register(),
        no_type_encoded_names::register(),
        timeout_on_io::register(),
        no_nullish_default_on_input::register(),
        prefer_switch_over_chained_if::register(),
        prefer_todo::register(),
        prefer_mock_promise_shorthand::register(),
        // @TODO: flagged:
        // src/rules/no_empty_test_file/text.rs:62:1: warning [no-multi-op-oneliner] Line has 11 chained operations — extract intermediate named bindings so each step's purpose is visible.
        // assert_eq!(run("utils.spec.ts", "// TODO: add tests").len(), 1); // comply-ignore: todo-needs-issue-link — test content, not a real marker.
        no_multi_op_oneliner::register(),
        // v1.2 — api-design + language-typescript rules
        no_enum::register(),
        no_double_cast::register(),
        no_json_parse_cast::register(),
        no_large_snapshots::register(),
        no_inline_function_event_listener::register(),
        no_inline_param_type::register(),
        no_interpolation_in_snapshots::register(),
        prefer_type_over_interface::register(),
        no_function_overloads::register(),
        no_verb_in_rest_url::register(),
        // v1.4 — ecosystem rules (security / testing / react / tanstack / zod / drizzle / tailwind)
        no_new_regex_with_variable::register(),
        no_auth_token_in_localstorage::register(),
        no_dangerously_set_inner_html::register(),
        no_unknown_property::register(),
        no_unsanitized_property::register(),
        no_hardcoded_secret::register(),
        no_focused_test::register(),
        no_done_callback::register(),
        no_match_snapshot::register(),
        react_no_array_index_key::register(),
        react_use_state_lazy_init::register(),
        react_no_and_conditional_jsx::register(),
        react_hoist_regex_outside_component::register(),
        react_hoist_static_jsx::register(),
        react_no_sequential_await_in_component::register(),
        react_prefer_react_cache::register(),
        tanstack_query_array_key::register(),
        tanstack_query_fn_must_throw_on_error::register(),
        tanstack_query_key_includes_params::register(),
        tanstack_query_no_cache_time::register(),
        tanstack_query_no_deprecated_props::register(),
        tanstack_query_no_enabled_true::register(),
        tanstack_query_no_is_loading::register(),
        tanstack_query_no_keep_previous_data_prop::register(),
        tanstack_query_no_query_callbacks::register(),
        tanstack_query_no_use_error_boundary::register(),
        tanstack_query_prefer_key_factory::register(),
        tanstack_query_prefer_query_options::register(),
        tanstack_query_prefer_suspense_query::register(),
        tanstack_query_require_stale_time::register(),
        xstate_spawn_usage::register(),
        zod_prefer_top_level_format::register(),
        zod_consistent_import_source::register(),
        zod_no_any::register(),
        zod_no_empty_custom_schema::register(),
        zod_no_number_schema_with_int::register(),
        zod_prefer_safe_parse::register(),
        zod_string_min_1_required::register(),
        zod_trim_before_min::register(),
        zod_prefer_discriminated_union::register(),
        zod_prefer_enum_over_literal_union::register(),
        zod_refine_requires_path::register(),
        zod_require_error_messages::register(),
        zod_no_optional_nullable_chain::register(),
        zod_no_transform_in_record_key::register(),
        zod_no_throw_in_refine::register(),
        zod_no_string_schema_with_uuid::register(),
        drizzle_timestamp_with_timezone::register(),
        tailwind_no_dynamic_class::register(),
        // v1.5 — Rust rules from the language-rust skill. All have clippy
        // coverage; these mod.rs files document them so `comply list` and
        // `comply explain` surface the mapping. See each rule's rust.rs
        // for the corresponding clippy lint name + setup.
        rust_no_unwrap::register(),
        rust_no_panic_macros::register(),
        rust_no_todo_macro::register(),
        rust_no_sleep_in_test::register(),
        rust_prefer_tracing_over_log::register(),
        // rust_must_use_on_result removed: std::result::Result is already
        // `#[must_use]` and type aliases (`io::Result`, `anyhow::Result`)
        // inherit it. Explicitly annotating Result-returning pub fns is
        // redundant and trips clippy::double_must_use. The rule's use case
        // collapsed down to hypothetical new types named Result that
        // don't alias std — we've never seen one in the wild.
        rust_undocumented_unsafe::register(),
        rust_await_holding_lock::register(),
        rust_large_enum_variant::register(),
        rust_ptr_arg::register(),
        // @TODO flagged:
        // /Users/rbaumier/www/comply/src/rules/no_unreadable_array_destructuring/typescript.rs:61:15: warning [rust-explicit-iter-loop] it is more concise to loop over references to containers instead of using explicit iteration methods
        // for &b in bytes.iter() {
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
        arrow_this_in_function::register(),
        no_side_effects_in_initialization::register(),
        // v2.8 — Comments: mechanical comment-quality rules.
        banned_comment_words::register(),
        no_section_divider_comments::register(),
        jsdoc_missing_example::register(),
        // eslint-plugin-jsdoc imports — 12 rules.
        jsdoc_check_property_names::register(),
        jsdoc_check_tag_names::register(),
        jsdoc_check_template_names::register(),
        jsdoc_check_types::register(),
        jsdoc_check_values::register(),
        jsdoc_valid_types::register(),
        jsdoc_require_param_description::register(),
        jsdoc_require_param_name::register(),
        jsdoc_require_returns_description::register(),
        jsdoc_require_hyphen_before_param_description::register(),
        jsdoc_require_property::register(),
        jsdoc_require_property_description::register(),
        jsdoc_require_property_name::register(),
        jsdoc_require_rejects::register(),
        jsdoc_require_yields::register(),
        jsdoc_require_yields_check::register(),
        jsdoc_require_tags::register(),
        jsdoc_require_template::register(),
        jsdoc_require_next_description::register(),
        jsdoc_require_template_description::register(),
        jsdoc_require_yields_description::register(),
        comment_paraphrases_code::register(),
        // v2.9 — Naming: intent + collection-type alignment.
        no_misleading_collection_name::register(),
        // v2.7+ — Framework rules (React + Vue).
        react_no_cookies_in_layout::register(),
        react_no_object_in_dep_array::register(),
        react_no_pass_data_to_parent::register(),
        react_no_reset_all_state_on_prop_change::register(),
        // RSC boundary rules — enforce server/client component contracts.
        react_no_client_hook_in_server_component::register(),
        react_no_event_handler_in_server_component::register(),
        react_no_browser_api_in_server_component::register(),
        react_no_class_component_in_server_component::register(),
        react_no_async_client_component::register(),
        react_no_server_only_in_client::register(),
        react_no_metadata_export_in_client::register(),
        react_no_generate_static_params_in_client::register(),
        react_no_next_headers_in_client::register(),
        react_duplicate_use_directive::register(),
        react_no_client_only_in_server_component::register(),
        react_layout_requires_children_prop::register(),
        react_no_find_dom_node::register(),
        vue_no_options_api::register(),
        vue_no_reactive_destructure::register(),
        vue_v_for_needs_stable_key::register(),
        vue_no_duplicate_v_if::register(),
        // Database rules (extracted from the database skill).
        sql_no_select_star::register(),
        sql_no_between_timestamp::register(),
        sql_no_offset_pagination::register(),
        sql_no_varchar::register(),
        sql_no_float_for_money::register(),
        sql_no_like_wildcard_prefix::register(),
        sql_no_pg_enum::register(),
        sql_prefer_exists_over_in::register(),
        db_no_n_plus_one::register(),
        db_no_string_concat_sql::register(),
        migration_needs_lock_timeout::register(),
        migration_needs_rollback::register(),
        drizzle_fk_needs_index::register(),
        // Testing rules (extracted from the testing skill).
        no_fire_event::register(),
        no_wait_for_timeout::register(),
        no_page_click_deprecated::register(),
        no_manual_rtl_cleanup::register(),
        no_mock_fetch_directly::register(),
        no_test_logic::register(),
        no_test_prefixes::register(),
        no_test_return_statement::register(),
        no_alias_methods::register(),
        prefer_spy_on::register(),
        // SonarJS-equivalent rules (native implementations).
        cognitive_complexity::register(),
        halstead_complexity::register(),
        no_identical_functions::register(),
        no_identical_title::register(),
        no_gratuitous_expression::register(),
        no_all_duplicated_branches::register(),
        no_redundant_assignment::register(),
        no_sort_without_comparator::register(),
        generator_without_yield::register(),
        no_equals_in_for_termination::register(),
        no_for_in_iterable::register(),
        no_function_declaration_in_block::register(),
        operation_returning_nan::register(),
        no_collapsible_if::register(),
        no_redundant_boolean::register(),
        block_scope_case::register(),
        prefer_single_boolean_return::register(),
        no_one_iteration_loop::register(),
        prefer_early_return::register(),
        no_valueof_field::register(),
        no_nested_template_literal::register(),
        prefer_called_exactly_once_with::register(),
        prefer_called_with::register(),
        prefer_expect_resolves::register(),
        prefer_immediate_return::register(),
        no_hardcoded_ip::register(),
        // @TODO: ça flagged:
        //  if text.contains("http://") || text.contains("https://")
        // ET aussi les http:// dans les commentaires
        no_clear_text_protocol::register(),
        no_eval::register(),
        // JSDoc description rule.
        jsdoc_needs_description::register(),
        // Text-based code-quality rules.
        no_try_promise::register(),
        no_unused_collection::register(),
        prefer_while::register(),
        prefer_less_than::register(),
        expression_complexity::register(),
        no_duplicate_string::register(),
        no_ignored_exceptions::register(),
        no_inverted_boolean_check::register(),
        no_nested_switch::register(),
        arguments_order::register(),
        array_callback_without_return::register(),
        assertions_in_tests::register(),
        comma_or_logical_or_case::register(),
        cyclomatic_complexity::register(),
        dead_export::register(),
        duplicate_export::register(),
        god_module::register(),
        elseif_without_else::register(),
        for_loop_increment_sign::register(),
        inconsistent_function_call::register(),
        index_of_compare_to_positive::register(),
        inverted_assertion_arguments::register(),
        jsx_no_leaked_render::register(),
        max_call_chain_depth::register(),
        max_union_size::register(),
        nested_control_flow::register(),
        no_arguments_usage::register(),
        no_array_constructor::register(),
        no_array_delete::register(),
        no_associative_arrays::register(),
        no_async_constructor::register(),
        no_bitwise_in_boolean::register(),
        no_built_in_override::register(),
        no_case_label_in_switch::register(),
        no_collection_size_mischeck::register(),
        no_confidential_logging::register(),
        no_constructor_side_effects::register(),
        no_duplicate_in_composite::register(),
        no_duplicated_branches::register(),
        no_dynamic_template::register(),
        no_element_overwrite::register(),
        no_hardcoded_secret_signature::register(),
        no_hook_setter_in_body::register(),
        no_identical_conditions::register(),
        no_identical_expressions::register(),
        no_ignored_return::register(),
        no_in_misuse::register(),
        no_incomplete_assertions::register(),
        no_inconsistent_returns::register(),
        no_incorrect_string_concat::register(),
        no_insecure_jwt::register(),
        // @TODO: flagged alors que non:
        //         fn is_function_decl(trimmed: &str) -> bool {
        //     // Rust
        //     if trimmed.starts_with("pub fn ")
        //         || trimmed.starts_with("pub async fn ")
        //         || trimmed.starts_with("fn ")
        //         || trimmed.starts_with("async fn ")
        //         || trimmed.starts_with("pub(crate) fn ")
        //     {
        //         return true;
        //     }
        //     // TypeScript/JavaScript
        //     if trimmed.starts_with("export function ")
        //         || trimmed.starts_with("export async function ")
        //         || trimmed.starts_with("export default function ")
        //         || trimmed.starts_with("function ")
        //         || trimmed.starts_with("async function ")
        //     {
        //         return true;
        //     }
        //     // Arrow/method patterns
        //     if (trimmed.contains("=> {") || trimmed.contains("=> ("))
        //         && (trimmed.starts_with("export const ") || trimmed.starts_with("const "))
        //     {
        //         return true;
        //     }
        //     false
        // }
        //
        // IDEM:
        //         pub fn is_rule_enabled(&self, rule_id: &str, file_path: &Path) -> bool {
        //     if let Some(rule) = self.raw.rules.get(rule_id)
        //         && rule.disabled == Some(true)
        //     {
        //         return false;
        //     }
        //     for idx in self.glob_matcher.matches(file_path) {
        //         if self.disable_lists[idx].iter().any(|d| d == rule_id) {
        //             return false;
        //         }
        //     }
        //     true
        // }
        no_invariant_returns::register(),
        no_misleading_array_reverse::register(),
        no_nested_assignment::register(),
        no_nested_functions::register(),
        no_post_message_star::register(),
        no_primitive_wrappers::register(),
        no_redundant_clsx::register(),
        no_redundant_jump::register(),
        no_redundant_optional::register(),
        no_return_type_any::register(),
        no_same_argument_assert::register(),
        no_small_switch::register(),
        no_undefined_argument::register(),
        no_undefined_assignment::register(),
        no_unenclosed_multiline_block::register(),
        no_uniq_key::register(),
        no_unthrown_error::register(),
        no_unverified_certificate::register(),
        no_useless_increment::register(),
        no_useless_intersection::register(),
        no_useless_react_setstate::register(),
        no_weak_cipher::register(),
        no_weak_hashing::register(),
        no_weak_keys::register(),
        no_weak_ssl::register(),
        no_xml_external_entity::register(),
        non_existent_operator::register(),
        prefer_default_last::register(),
        prefer_object_literal::register(),
        prefer_promise_shorthand::register(),
        prefer_type_guard::register(),
        public_static_readonly::register(),
        redundant_type_aliases::register(),
        strings_comparison::register(),
        test_check_exception::register(),
        use_type_alias::register(),
        useless_string_operation::register(),
        no_deprecated_api::register(),
        no_deprecated_cipher::register(),
        no_ecb_mode::register(),
        no_electron_node_integration::register(),
        no_empty_test_file::register(),
        no_globals_shadowing::register(),
        no_implicit_deps::register(),
        no_extraneous_import::register(),
        no_unsupported_node_builtins::register(),
        file_extension_in_import::register(),
        no_loop_counter_reassign::register(),
        no_misplaced_loop_counter::register(),
        no_nested_incdec::register(),
        no_unverified_hostname::register(),
        prefer_destructuring_assignment::register(),
        prefer_regexp_exec::register(),
        regex_anchor_precedence::register(),
        regex_complexity::register(),
        regex_no_control_chars::register(),
        regex_no_duplicate_chars::register(),
        regex_no_empty_after_reluctant::register(),
        regex_no_empty_alternative::register(),
        regex_no_empty_character_class::register(),
        regex_no_empty_group::register(),
        regex_no_empty_string_match::register(),
        regex_no_misleading_char_class::register(),
        regex_no_multiple_spaces::register(),
        regex_no_single_char_class::register(),
        regex_no_slow_pattern::register(),
        regex_no_stateful_global::register(),
        regex_no_unused_groups::register(),
        regex_prefer_char_class::register(),
        // @TODO
        //         /Users/rbaumier/www/diff-review
        // ❯ ~/www/comply/target/release/comply

        // thread '<unnamed>' (63760245) panicked at src/rules/regex_prefer_quantifier/text.rs:47:33:
        // byte index 43 is not a char boundary; it is inside 'é' (bytes 42..44) of `* REVIEW: les boutons sont tout le temps déclarés, on ne peut pas utiliser le composant Button ? *`
        // note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        // thread '<unnamed>' (63760245) panicked at src/rules/regex_prefer_quantifier/text.rs:47:33:
        // byte index 46 is not a char boundary; it is inside '—' (bytes 45..48) of `** Whitelist of languages supported by Shiki — prevents loading arbitrary grammars *`

        // thread '<unnamed>' (63760242) panicked at src/rules/regex_prefer_quantifier/text.rs:47:33:
        // byte index 46 is not a char boundary; it is inside '—' (bytes 45..48) of `** Create a user with an active organization — ready for org-scoped operations. *`

        // thread '<unnamed>' (63760247) panicked at src/rules/regex_prefer_quantifier/text.rs:47:33:
        // byte index 24 is not a char boundary; it is inside '—' (bytes 23..26) of `** Service return type — dates are Date objects, Hono serializes to ISO strings *`

        // thread '<unnamed>' (63760242) panicked at src/rules/regex_prefer_quantifier/text.rs:47:33:
        // byte index 11 is not a char boundary; it is inside '—' (bytes 10..13) of `** DB row — `created_at` is a Date from Drizzle, API schema uses ISO string *`
        // ❯ ~/www/comply/target/release/comply src

        // thread '<unnamed>' (63760600) panicked at src/rules/regex_prefer_quantifier/text.rs:47:33:
        // byte index 43 is not a char boundary; it is inside 'é' (bytes 42..44) of `* REVIEW: les boutons sont tout le temps déclarés, on ne peut pas utiliser le composant Button ? *`
        // note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

        // thread '<unnamed>' (63760600) panicked at src/rules/regex_prefer_quantifier/text.rs:47:33:
        // byte index 46 is not a char boundary; it is inside '—' (bytes 45..48) of `** Whitelist of languages supported by Shiki — prevents loading arbitrary grammars *`

        // thread '<unnamed>' (63760602) panicked at src/rules/regex_prefer_quantifier/text.rs:47:33:
        // byte index 11 is not a char boundary; it is inside '—' (bytes 10..13) of `** DB row — `created_at` is a Date from Drizzle, API schema uses ISO string *`

        // thread '<unnamed>' (63760602) panicked at src/rules/regex_prefer_quantifier/text.rs:47:33:
        // byte index 24 is not a char boundary; it is inside '—' (bytes 23..26) of `** Service return type — dates are Date objects, Hono serializes to ISO strings *`
        regex_prefer_quantifier::register(),
        regex_use_unicode_flag::register(),
        regex_no_octal::register(),
        regex_no_escape_backspace::register(),
        regex_sort_flags::register(),
        regex_no_useless_two_nums_quantifier::register(),
        regex_no_zero_quantifier::register(),
        regex_no_obscure_range::register(),
        regex_no_useless_lazy::register(),
        regex_no_empty_lookaround::register(),
        regex_no_standalone_backslash::register(),
        regex_no_invisible_character::register(),
        regex_no_contradiction_with_assertion::register(),
        regex_no_dupe_disjunctions::register(),
        regex_no_misleading_capturing_group::register(),
        regex_no_missing_g_flag::register(),
        regex_no_optional_assertion::register(),
        regex_no_potentially_useless_backreference::register(),
        regex_no_super_linear_move::register(),
        regex_no_useless_assertions::register(),
        regex_no_useless_backreference::register(),
        regex_no_useless_dollar_replacements::register(),
        regex_confusing_quantifier::register(),
        regex_no_empty_string_literal_v::register(),
        regex_no_extra_lookaround_assertions::register(),
        regex_no_legacy_features::register(),
        regex_no_non_standard_flag::register(),
        regex_no_trivially_nested_assertion::register(),
        regex_no_trivially_nested_quantifier::register(),
        regex_no_useless_flag::register(),
        regex_no_useless_quantifier::register(),
        regex_no_useless_set_operand::register(),
        regex_no_useless_string_literal::register(),
        regex_optimal_lookaround_quantifier::register(),
        regex_prefer_predefined_assertion::register(),
        regex_prefer_set_operation::register(),
        jsdoc_informative_docs::register(),
        jsdoc_reject_any_type::register(),
        jsdoc_reject_function_type::register(),
        // eslint-plugin-security rules (native implementations).
        no_bidi_characters::register(),
        no_timing_attack::register(),
        no_disable_mustache_escape::register(),
        // eslint-plugin-functional rules (native implementations).
        no_class_inheritance::register(),
        no_promise_reject::register(),
        no_try_statements::register(),
        too_many_break_or_continue::register(),
        hono_cookie_no_httponly::register(),
        hono_cookie_no_samesite::register(),
        hono_cookie_no_secure::register(),
        hono_cors_permissive::register(),
        hono_csp_unsafe::register(),
        hono_csrf_missing::register(),
        hono_jwt_secret_hardcoded::register(),
        hono_missing_secure_headers::register(),
        hono_no_get_with_body::register(),
        hono_no_hardcoded_cors_origin::register(),
        hono_secure_headers_disabled::register(),
        // Angular rules.
        angular_no_topromise::register(),
        angular_prefer_signals::register(),
        angular_require_onpush::register(),
        // Prisma rules.
        prisma_no_findmany_without_take::register(),
        prisma_no_nested_include_depth::register(),
        prisma_require_transaction_for_multi_write::register(),
        html_no_abstract_roles::register(),
        html_no_aria_hidden_body::register(),
        html_no_duplicate_attrs::register(),
        html_no_duplicate_id::register(),
        html_no_nested_interactive::register(),
        html_no_non_scalable_viewport::register(),
        html_no_obsolete_tags::register(),
        html_no_positive_tabindex::register(),
        html_no_script_style_type::register(),
        html_no_skip_heading_levels::register(),
        html_prefer_https::register(),
        html_require_button_type::register(),
        html_require_closing_tags::register(),
        html_require_doctype::register(),
        html_require_explicit_size::register(),
        html_require_img_alt::register(),
        html_require_input_label::register(),
        html_require_meta_charset::register(),
        html_require_title::register(),
        api_first::register(),
        auth_on_mutation::register(),
        data_clumps::register(),
        error_message_is_remediation::register(),
        factory_di_shape::register(),
        filename_naming_convention::register(),
        intermediate_variables::register(),
        justify_inaction::register(),
        no_inferred_any::register(),
        no_logger_in_business_logic::register(),
        no_raw_db_entity_in_handler::register(),
        option_vs_result::register(),
        pure_by_default::register(),
        structured_api_error::register(),
        symmetric_pairs::register(),
        a11y_alt_text::register(),
        a11y_anchor_ambiguous_text::register(),
        a11y_anchor_has_content::register(),
        a11y_anchor_is_valid::register(),
        a11y_aria_activedescendant_has_tabindex::register(),
        a11y_aria_props::register(),
        a11y_aria_role::register(),
        a11y_aria_unsupported_elements::register(),
        a11y_autocomplete_valid::register(),
        a11y_click_events_have_key_events::register(),
        a11y_control_has_associated_label::register(),
        a11y_heading_has_content::register(),
        a11y_html_has_lang::register(),
        a11y_iframe_has_title::register(),
        a11y_img_redundant_alt::register(),
        a11y_interactive_supports_focus::register(),
        a11y_label_has_associated_control::register(),
        a11y_media_has_caption::register(),
        a11y_mouse_events_have_key_events::register(),
        a11y_no_access_key::register(),
        a11y_no_aria_hidden_on_focusable::register(),
        a11y_no_autofocus::register(),
        a11y_no_distracting_elements::register(),
        a11y_no_interactive_element_to_noninteractive_role::register(),
        a11y_no_noninteractive_element_interactions::register(),
        a11y_no_noninteractive_element_to_interactive_role::register(),
        a11y_no_noninteractive_tabindex::register(),
        a11y_no_redundant_roles::register(),
        a11y_no_static_element_interactions::register(),
        a11y_prefer_tag_over_role::register(),
        a11y_role_has_required_aria_props::register(),
        a11y_scope::register(),
        a11y_tabindex_no_positive::register(),
        // eslint-plugin-import rules (native implementations).
        imports_first::register(),
        max_dependencies::register(),
        newline_after_import::register(),
        no_absolute_path::register(),
        no_duplicate_imports::register(),
        no_import_dist::register(),
        no_import_module_exports::register(),
        no_import_node_modules_by_path::register(),
        no_import_node_test::register(),
        no_mocks_import::register(),
        no_mutable_exports::register(),
        no_useless_path_segments::register(),
        no_self_import::register(),
        no_unassigned_import::register(),
        import_no_commonjs::register(),
        import_default::register(),
        import_export::register(),
        import_named::register(),
        import_namespace::register(),
        import_no_cycle::register(),
        type_only_dependency::register(),
        unlisted_dependency::register(),
        unused_dependency::register(),
        unused_file::register(),
        import_no_duplicates::register(),
        import_no_named_as_default::register(),
        import_no_unresolved::register(),
        import_no_amd::register(),
        import_no_webpack_loader_syntax::register(),
        import_no_empty_named_blocks::register(),
        import_no_dynamic_require::register(),
        require_not_empty::register(),
        import_dynamic_import_chunkname::register(),
        import_consistent_type_specifier_style::register(),
        exports_last::register(),
        // eslint-plugin-unicorn rules (native implementations).
        catch_error_name::register(),
        consistent_date_clone::register(),
        consistent_destructuring::register(),
        consistent_empty_array_spread::register(),
        consistent_existence_index_check::register(),
        consistent_function_scoping::register(),
        consistent_template_literal_escape::register(),
        custom_error_definition::register(),
        empty_brace_spaces::register(),
        error_message::register(),
        escape_case::register(),
        expiring_todo_comments::register(),
        explicit_length_check::register(),
        new_for_builtins::register(),
        no_abusive_eslint_disable::register(),
        no_accessor_recursion::register(),
        no_anonymous_default_export::register(),
        no_array_callback_reference::register(),
        no_array_method_this_argument::register(),
        no_array_reduce::register(),
        no_array_reverse::register(),
        no_array_sort_mutation::register(),
        no_assign_mutated_array::register(),
        no_await_expression_member::register(),
        no_await_in_promise_methods::register(),
        no_console_spaces::register(),
        no_document_cookie::register(),
        no_document_domain::register(),
        no_document_write::register(),
        no_inner_html::register(),
        no_unsafe_alloc::register(),
        no_unsafe_shell_exec::register(),
        detect_dangerous_redirects::register(),
        detect_option_rejectunauthorized::register(),
        react_no_javascript_urls::register(),
        no_empty_catch::register(),
        no_empty_file::register(),
        no_extra_arguments::register(),
        no_for_loop::register(),
        no_hex_escape::register(),
        no_immediate_mutation::register(),
        no_instanceof_builtins::register(),
        no_invalid_fetch_options::register(),
        no_invalid_remove_event_listener::register(),
        no_keyword_prefix::register(),
        no_lonely_if::register(),
        no_delete::register(),
        no_let::register(),
        no_magic_array_flat_depth::register(),
        no_mutating_assign::register(),
        no_mutating_methods::register(),
        no_mutation::register(),
        no_named_default::register(),
        no_negated_condition::register(),
        no_negation_in_equality_check::register(),
        no_null::register(),
        no_object_as_default_parameter::register(),
        no_process_exit::register(),
        no_single_promise_in_promise_methods::register(),
        no_static_only_class::register(),
        no_thenable::register(),
        no_this_assignment::register(),
        no_typeof_undefined::register(),
        no_unnecessary_array_flat_depth::register(),
        no_unnecessary_array_splice_count::register(),
        no_unnecessary_await::register(),
        no_unnecessary_slice_end::register(),
        no_unreadable_array_destructuring::register(),
        no_unreadable_iife::register(),
        no_useless_collection_argument::register(),
        no_useless_error_capture_stack_trace::register(),
        no_useless_fallback_in_spread::register(),
        no_useless_iterator_to_array::register(),
        no_useless_length_check::register(),
        no_useless_promise_resolve_reject::register(),
        no_useless_spread::register(),
        no_useless_switch_case::register(),
        no_zero_fractions::register(),
        number_literal_case::register(),
        numeric_separators_style::register(),
        prefer_add_event_listener::register(),
        prefer_array_find::register(),
        prefer_array_flat::register(),
        prefer_array_index_of::register(),
        prefer_array_some::register(),
        prefer_at::register(),
        prefer_bigint_literals::register(),
        prefer_blob_reading_methods::register(),
        prefer_class_fields::register(),
        prefer_classlist_toggle::register(),
        prefer_code_point::register(),
        prefer_date_now::register(),
        prefer_default_parameters::register(),
        prefer_dom_node_append::register(),
        prefer_dom_node_dataset::register(),
        prefer_dom_node_remove::register(),
        prefer_dom_node_text_content::register(),
        prefer_event_target::register(),
        prefer_export_from::register(),
        prefer_global_this::register(),
        prefer_import_meta_properties::register(),
        prefer_includes::register(),
        prefer_json_parse_buffer::register(),
        prefer_keyboard_event_key::register(),
        prefer_lazy_load::register(),
        prefer_logical_operator_over_ternary::register(),
        prefer_math_min_max::register(),
        prefer_math_trunc::register(),
        prefer_mock_return_shorthand::register(),
        prefer_modern_dom_apis::register(),
        prefer_modern_math_apis::register(),
        prefer_module::register(),
        prefer_native_coercion_functions::register(),
        prefer_negative_index::register(),
        prefer_node_protocol::register(),
        prefer_number_properties::register(),
        prefer_object_from_entries::register(),
        prefer_optional_catch_binding::register(),
        prefer_prototype_methods::register(),
        prefer_query_selector::register(),
        prefer_reflect_apply::register(),
        prefer_regexp_test::register(),
        prefer_response_static_json::register(),
        prefer_set_has::register(),
        prefer_set_size::register(),
        prefer_single_call::register(),
        prefer_spread::register(),
        prefer_static_regex::register(),
        prefer_string_raw::register(),
        prefer_string_replace_all::register(),
        prefer_string_slice::register(),
        prefer_string_starts_ends_with::register(),
        prefer_string_trim_start_end::register(),
        prefer_structured_clone::register(),
        prefer_ternary::register(),
        prefer_to_have_length::register(),
        prefer_top_level_await::register(),
        prefer_type_error::register(),
        relative_url_style::register(),
        require_array_join_separator::register(),
        require_explicit_undefined::register(),
        require_hook::register(),
        require_module_attributes::register(),
        require_module_specifiers::register(),
        require_number_to_fixed_digits_argument::register(),
        require_path_exists::register(),
        require_post_message_target_origin::register(),
        require_to_throw_message::register(),
        require_too_many_arguments::register(),
        switch_case_braces::register(),
        switch_case_break_position::register(),
        template_indent::register(),
        text_encoding_identifier_case::register(),
        throw_new_error::register(),
        // eslint-plugin-n (Node.js) rules.
        node_no_path_concat::register(),
        node_no_sync::register(),
        node_prefer_promises_fs::register(),
        node_prefer_promises_dns::register(),
        node_no_callback_literal::register(),
        node_handle_callback_err::register(),
        node_no_new_require::register(),
        node_no_process_env::register(),
        node_callback_return::register(),
        node_global_require::register(),
        node_no_mixed_requires::register(),
        node_hashbang::register(),
        node_no_exports_assign::register(),
        node_no_top_level_await::register(),
        node_no_unhandled_rejection::register(),
        node_prefer_stream_pipeline::register(),
        // NestJS rules (native implementations).
        nestjs_controller_return_type::register(),
        nestjs_no_entity_in_controller::register(),
        nestjs_no_forwardref_abuse::register(),
        // Next.js rules (native implementations).
        next_no_redirect_in_try_catch::register(),
        // eslint-plugin-react rules (native implementations).
        react_no_unstable_nested_components::register(),
        react_no_constructed_context_values::register(),
        react_no_object_type_as_default_prop::register(),
        react_no_danger_with_children::register(),
        react_void_dom_elements_no_children::register(),
        react_jsx_no_duplicate_props::register(),
        react_jsx_no_comment_textnodes::register(),
        display_name::register(),
        jsx_handler_names::register(),
        jsx_fragments::register(),
        react_no_render_return_value::register(),
        react_style_prop_object::register(),
        react_jsx_no_target_blank::register(),
        react_jsx_no_script_url::register(),
        react_iframe_missing_sandbox::register(),
        react_checked_requires_onchange::register(),
        react_no_this_in_sfc::register(),
        react_async_server_action::register(),
        react_no_access_state_in_setstate::register(),
        react_button_has_type::register(),
        react_jsx_key::register(),
        react_jsx_no_useless_fragment::register(),
        react_jsx_pascal_case::register(),
        react_jsx_props_no_spread_multi::register(),
        react_no_children_prop::register(),
        react_no_namespace::register(),
        react_no_string_refs::register(),
        react_no_unescaped_entities::register(),
        react_self_closing_comp::register(),
        react_no_invalid_html_attribute::register(),
        react_no_adjacent_inline_elements::register(),
        react_forward_ref_uses_ref::register(),
        react_no_typos::register(),
        react_jsx_no_bind::register(),
        react_jsx_no_jsx_as_prop::register(),
        react_jsx_no_new_array_as_prop::register(),
        react_jsx_no_new_object_as_prop::register(),
        jsx_no_new_function_as_prop::register(),
        jsx_no_undef::register(),
        jsx_ensure_booleans::register(),
        react_hook_form_destructuring_formstate::register(),
        react_no_chain_state_updates::register(),
        no_submit_handler_without_prevent_default::register(),
        no_sync_scripts::register(),
        // typescript-eslint rules (native implementations).
        ts_no_const_enum::register(),
        ts_no_duplicate_enum_values::register(),
        ts_no_extra_non_null_assertion::register(),
        ts_no_non_null_asserted_optional_chain::register(),
        ts_no_wrapper_object_types::register(),
        ts_no_unsafe_declaration_merging::register(),
        ts_no_misused_new::register(),
        ts_no_mixed_types::register(),
        ts_no_empty_object_type::register(),
        ts_no_export_equal::register(),
        ts_no_non_null_asserted_nullish_coalescing::register(),
        ts_no_confusing_non_null_assertion::register(),
        ts_no_unnecessary_type_constraint::register(),
        ts_no_inferrable_types::register(),
        ts_prefer_literal_enum_member::register(),
        ts_no_useless_empty_export::register(),
        ts_no_namespace::register(),
        ts_max_params::register(),
        ts_no_redeclare::register(),
        ts_no_restricted_imports::register(),
        ts_no_restricted_types::register(),
        ts_no_shadow::register(),
        ts_no_unused_expressions::register(),
        ts_no_unused_private_class_members::register(),
        ts_no_unused_vars::register(),
        ts_no_use_before_define::register(),
        ts_triple_slash_reference::register(),
        ts_unified_signatures::register(),
        ts_method_signature_style::register(),
        ts_member_ordering::register(),
        ts_init_declarations::register(),
        ts_class_methods_use_this::register(),
        ts_no_array_constructor::register(),
        ts_no_dupe_class_members::register(),
        ts_no_invalid_this::register(),
        ts_no_loop_func::register(),
        ts_no_magic_numbers::register(),
        ts_no_dynamic_delete::register(),
        ts_no_empty_function::register(),
        ts_no_extraneous_class::register(),
        ts_no_implicit_any_catch::register(),
        ts_no_import_type_side_effects::register(),
        ts_no_invalid_void_type::register(),
        ts_no_this_alias::register(),
        ts_no_unnecessary_parameter_property_assignment::register(),
        ts_no_useless_constructor::register(),
        ts_parameter_properties::register(),
        ts_prefer_for_of::register(),
        ts_prefer_function_type::register(),
        ts_adjacent_overload_signatures::register(),
        ts_ban_ts_comment::register(),
        ts_ban_tslint_comment::register(),
        ts_class_literal_property_style::register(),
        ts_consistent_generic_constructors::register(),
        ts_consistent_indexed_object_style::register(),
        ts_consistent_type_assertions::register(),
        ts_consistent_type_exports::register(),
        ts_consistent_type_imports::register(),
        ts_no_non_null_assertion::register(),
        ts_only_throw_error::register(),
        ts_prefer_promise_reject_errors::register(),
        ts_default_param_last::register(),
        ts_explicit_function_return_type::register(),
        ts_explicit_member_accessibility::register(),
        ts_explicit_module_boundary_types::register(),
        ts_init_declarations::register(),
        // eslint-plugin-playwright rules (native implementations).
        playwright_no_force_option::register(),
        playwright_no_page_pause::register(),
        playwright_no_networkidle::register(),
        playwright_no_element_handle::register(),
        playwright_no_eval::register(),
        vitest_hoisted_apis_on_top::register(),
        vitest_no_disabled_tests::register(),
        playwright_prefer_web_first_assertions::register(),
        playwright_no_unsafe_references::register(),
        playwright_no_raw_locators::register(),
        playwright_no_conditional_expect::register(),
        playwright_prefer_native_locators::register(),
        playwright_expect_expect::register(),
        playwright_max_expects::register(),
        playwright_max_nested_describe::register(),
        playwright_no_commented_out_tests::register(),
        playwright_no_conditional_in_test::register(),
        playwright_no_duplicate_hooks::register(),
        playwright_no_hooks::register(),
        playwright_no_nested_step::register(),
        playwright_no_nth_methods::register(),
        playwright_no_skipped_test::register(),
        playwright_no_standalone_expect::register(),
        playwright_no_useless_await::register(),
        playwright_no_useless_not::register(),
        playwright_no_wait_for_selector::register(),
        playwright_no_wait_for_navigation::register(),
        playwright_no_wait_for_timeout::register(),
        playwright_prefer_comparison_matcher::register(),
        playwright_prefer_equality_matcher::register(),
        playwright_prefer_hooks_in_order::register(),
        playwright_prefer_hooks_on_top::register(),
        playwright_prefer_strict_equal::register(),
        playwright_prefer_to_be::register(),
        playwright_prefer_to_contain::register(),
        playwright_prefer_to_have_count::register(),
        // eslint-plugin-jsdoc rules (native implementations).
        jsdoc_complete_sentence::register(),
        // eslint-plugin-de-morgan (native implementation).
        de_morgan_simplify::register(),
        // eslint-plugin-react-refresh (native implementation).
        react_refresh_only_export_components::register(),
        // eslint-plugin-playwright (native implementation).
        playwright_missing_await::register(),
        // eslint-plugin-better-tailwindcss rules.
        tailwind_no_duplicate_classes::register(),
        tailwind_no_conflicting_classes::register(),
        // package-json rules.
        package_json_sorted_deps::register(),
        package_json_unique_deps::register(),
        no_index_file::register(),
        top_level_function::register(),
        comment_prose_quality::register(),
        // architecture: hexagonal layer boundaries.
        layer_import_boundary::register(),
        // v2.12 — rules derived from code-review feedback.
        rust_no_as_numeric_cast::register(),
        rust_prefer_strum::register(),
        rust_prefer_unwrap_or_explicit::register(),
        rust_constants_top_of_file::register(),
        rust_explicit_enum_match_arms::register(),
        rust_duration_over_integer_with_unit::register(),
        sql_index_needs_rationale_comment::register(),
        // v2.13 — native id-length (replaces oxlint + clippy delegation)
        // so the diagnostic names the offending identifier.
        id_length::register(),
        // v3.0 — Skill-driven rules: Batch 1 (TypeScript/Architecture)
        avoid_barrel_files::register(),
        avoid_re_export_all::register(),
        avoid_importing_barrel_files::register(),
        import_dedupe::register(),
        no_full_import::register(),
        no_test_imports_in_prod::register(),
        no_default_export::register(),
        prefer_promise_all::register(),
        ts_prefer_using_declaration::register(),
        // v3.0 — Skill-driven rules: Batch 2 (React)
        react_server_action_requires_validation::register(),
        react_server_action_requires_auth::register(),
        react_prefer_use_transition::register(),
        react_no_initialize_state_in_effect::register(),
        react_no_inline_default_prop::register(),
        react_passive_event_listeners::register(),
        react_no_derived_state_in_effect::register(),
        react_no_empty_effect::register(),
        react_use_state_initializer_function::register(),
        // v3.0 — Skill-driven rules: Batch 3 (Tailwind)
        tailwind_no_important_modifier::register(),
        tailwind_no_arbitrary_z_index::register(),
        tailwind_enforces_negative_arbitrary_values::register(),
        tailwind_prefer_size_shorthand::register(),
        tailwind_classnames_order::register(),
        tailwind_no_deprecated_classes::register(),
        tailwind_prefer_shorthand::register(),
        tailwind_no_apply_for_variants::register(),
        tailwind_prefer_cn_utility::register(),
        tailwind_no_unnecessary_whitespace::register(),
        // v3.0 — Skill-driven rules: Batch 4 (SQL/Database)
        sql_create_index_concurrently::register(),
        sql_nullable_requires_comment::register(),
        sql_advisory_lock_prefer_xact::register(),
        sql_require_transaction_timeout::register(),
        // v3.0 — Skill-driven rules: Batch 5 (Rust)
        rust_prefer_once_lock::register(),
        rust_vec_with_capacity::register(),
        rust_prefer_channel_over_arc_mutex_vec::register(),
        rust_anyhow_context_on_question_mark::register(),
        rust_must_use_on_result_fn::register(),
        rust_unsafe_ffi_isolation::register(),
        rust_thiserror_for_lib::register(),
        // v3.0 — Skill-driven rules: Batch 6 (TanStack Start)
        tanstack_start_server_fn_requires_validation::register(),
        tanstack_start_server_fn_requires_auth::register(),
        tanstack_start_server_fn_file_convention::register(),
        tanstack_start_require_validate_search::register(),
        // v3.0 — Skill-driven rules: Batch 8 (API Design)
        api_no_array_root_response::register(),
        api_list_requires_pagination::register(),
        api_import_from_public_index::register(),
        api_no_boolean_field_in_response::register(),
        api_deprecation_headers::register(),
        // v3.0 — Skill-driven rules: Batch 10 (Vue)
        vue_script_setup_required::register(),
        vue_sfc_section_order::register(),
        vue_no_v_html_unsafe::register(),
        vue_prefer_v_else::register(),
        vue_require_lifecycle_cleanup::register(),
        vue_pinia_store_to_refs::register(),
        vue_define_emits_typed::register(),
        vue_prefer_computed::register(),
        vue_markraw_for_third_party::register(),
        vue_url_state_for_filters::register(),
        // v3.0 — Skill-driven rules: Batch 11 (i18n)
        i18n_json_identical_keys::register(),
        i18n_json_identical_placeholders::register(),
        i18n_json_no_empty_values::register(),
        i18n_json_no_nesting::register(),
        i18n_json_no_untranslated::register(),
        i18n_json_valid_message_syntax::register(),
        i18n_no_hardcoded_string_in_jsx::register(),
        i18n_no_concat_translation_key::register(),
        i18n_no_string_concat_with_translation::register(),
        i18n_prefer_intl_api::register(),
        i18n_no_manual_pluralization::register(),
        // v3.0 — Skill-driven rules: Batch 12 (security)
        audit_log_required_fields::register(),
        no_error_details_in_response::register(),
        no_mass_assignment::register(),
        no_open_redirect::register(),
        no_path_traversal::register(),
        no_property_mutation::register(),
        no_prototype_pollution::register(),
        no_shell_exec::register(),
        no_ssrf_fetch::register(),
        no_unvalidated_url_redirect::register(),
        // v3.0 — Skill-driven rules: Batch 13 (better-auth)
        better_auth_no_disable_csrf::register(),
        better_auth_no_disable_origin_check::register(),
        better_auth_plugin_import_path::register(),
        better_auth_require_rate_limit::register(),
        better_auth_trusted_providers::register(),
        // v3.0 — Skill-driven rules: Batch 14 (testing)
        testing_no_and_in_test_name::register(),
        testing_no_undefined_mock_var::register(),
        testing_prefer_msw::register(),
        testing_prefer_test_each::register(),
        // v3.0 — Skill-driven rules: Batch 15 (Drizzle ORM)
        drizzle_chunk_large_batch_insert::register(),
        drizzle_no_select_without_limit::register(),
        drizzle_no_sql_raw_with_variable::register(),
        drizzle_returning_on_insert_update::register(),
        drizzle_zod_prefer_generated_schema::register(),
        elysia_after_response_mutation::register(),
        elysia_apollo_no_req_res::register(),
        elysia_apollo_playground_prod::register(),
        elysia_array_no_bounds::register(),
        elysia_bearer_missing_www_auth::register(),
        elysia_bearer_not_validated::register(),
        elysia_bearer_strip_typo::register(),
        elysia_better_auth_basepath::register(),
        elysia_better_auth_mount::register(),
        elysia_better_auth_null_session::register(),
        elysia_booleanstring_for_body::register(),
        elysia_cf_compile_required::register(),
        elysia_cf_env_import::register(),
        elysia_cf_no_inline_values::register(),
        elysia_cf_no_static_plugin::register(),
        elysia_cookie_getter_setter::register(),
        elysia_cookie_no_httponly::register(),
        elysia_cookie_no_samesite::register(),
        elysia_cookie_no_secure::register(),
        elysia_cookie_removal_api::register(),
        elysia_cookie_signed_no_secrets::register(),
        elysia_cors_allowed_headers_wildcard::register(),
        elysia_cors_credentials_wildcard::register(),
        elysia_cors_methods_wildcard::register(),
        elysia_cors_regex_unanchored::register(),
        elysia_cors_wildcard::register(),
        elysia_cron_name_required::register(),
        elysia_cron_timezone::register(),
        elysia_custom_errors_in_model::register(),
        elysia_deno_serve_fetch::register(),
        elysia_deploy_no_graceful_shutdown::register(),
        elysia_deploy_no_health::register(),
        elysia_deploy_port_hardcoded::register(),
        elysia_deploy_prod_no_aot::register(),
        elysia_derive_validated_data::register(),
        elysia_drizzle_intermediate_var::register(),
        elysia_eden_error_unchecked::register(),
        elysia_eden_null_body::register(),
        elysia_eden_server_export_type::register(),
        elysia_file_magic_number::register(),
        elysia_file_upload_no_maxsize::register(),
        elysia_file_upload_no_type::register(),
        elysia_global_with_types::register(),
        elysia_graphql_yoga_context::register(),
        elysia_group_deep_paths::register(),
        elysia_guard_derive_no_headers::register(),
        elysia_headers_lowercase::register(),
        elysia_heavy_onrequest::register(),
        elysia_hooks_before_routes::register(),
        elysia_html_import_uppercase::register(),
        elysia_html_xss_no_safe::register(),
        elysia_import_t_not_typebox::register(),
        elysia_inline_handlers::register(),
        elysia_jwt_cookie_no_httponly::register(),
        elysia_jwt_missing_exp::register(),
        elysia_jwt_name_multiple::register(),
        elysia_jwt_secret_hardcoded::register(),
        elysia_jwt_verify_unchecked::register(),
        elysia_listen_callback_info::register(),
        elysia_listen_port_type::register(),
        elysia_macro_named_inference::register(),
        elysia_macro_throw_status::register(),
        elysia_mapresponse_sync_compression::register(),
        elysia_model_export_types::register(),
        elysia_named_plugin::register(),
        elysia_nextjs_typeof_process::register(),
        elysia_no_body_on_get::register(),
        elysia_no_context_type::register(),
        elysia_no_mix_zod_typebox::register(),
        elysia_no_server_assertion::register(),
        elysia_nodejs_adapter_required::register(),
        elysia_numeric_body_no_coerce::register(),
        elysia_numeric_no_bounds::register(),
        elysia_objectstring_for_query::register(),
        elysia_onerror_missing_validation::register(),
        elysia_onparse_no_content_type::register(),
        elysia_openapi_from_types_prod::register(),
        elysia_openapi_security_scheme::register(),
        elysia_otel_named_functions::register(),
        elysia_plugin_functional_callback::register(),
        elysia_prefer_instance_plugin::register(),
        elysia_prefer_redirect::register(),
        elysia_prefer_status_over_set::register(),
        elysia_require_method_chaining::register(),
        elysia_resolve_outside_guard::register(),
        elysia_response_keyed_by_status::register(),
        elysia_response_status_mismatch::register(),
        elysia_route_all_method::register(),
        elysia_route_missing_auth::register(),
        elysia_route_missing_body_schema::register(),
        elysia_route_missing_params_schema::register(),
        elysia_route_missing_response_schema::register(),
        elysia_scope_missing::register(),
        elysia_server_timing_prod::register(),
        elysia_service_coupled::register(),
        elysia_service_return_not_throw::register(),
        elysia_static_await_hmr::register(),
        elysia_static_inline_value::register(),
        elysia_streaming_headers_after_yield::register(),
        elysia_string_format_email::register(),
        elysia_test_listen_not_handle::register(),
        elysia_test_missing_401::register(),
        elysia_test_missing_validation::register(),
        elysia_transform_no_schema::register(),
        elysia_ws_connection_leak::register(),
        elysia_ws_headers_unvalidated::register(),
        elysia_ws_missing_auth::register(),
        elysia_ws_subscribe_before_publish::register(),
        elysia_zod_coerce_params::register(),
        enforce_delete_with_where::register(),
        enforce_update_with_where::register(),
        pg_require_limit::register(),
        // v3.1 — Skill-driven rules: Batch 16 (mixed: security, i18n, vue, rust, tailwind, testing)
        better_auth_middleware_requires_headers::register(),
        better_auth_require_secure_cookies::register(),
        express_session_require_name::register(),
        drizzle_no_push_in_production::register(),
        i18n_no_unnecessary_trans_component::register(),
        i18n_prefer_logical_css_properties::register(),
        no_conditional_async_return::register(),
        no_conditional_tests::register(),
        no_unchecked_json_parse::register(),
        no_unsanitized_method::register(),
        rust_prefer_fast_hasher::register(),
        rust_prefer_cow::register(),
        rust_no_mutex_in_single_threaded::register(),
        tailwind_no_magic_spacing::register(),
        tailwind_read_theme_before_classes::register(),
        tanstack_start_loader_stale_time::register(),
        tanstack_start_no_client_import_in_server_fn::register(),
        serialize_javascript_no_unsafe::register(),
        testing_no_real_external_service::register(),
        ts_prefer_satisfies::register(),
        valid_describe_callback::register(),
        vue_no_mutate_prop::register(),
        xstate_entry_exit_action::register(),
        xstate_event_names::register(),
        xstate_invoke_usage::register(),
        xstate_no_async_guard::register(),
        xstate_no_imperative_action::register(),
        xstate_no_infinite_loop::register(),
        xstate_no_inline_implementation::register(),
        xstate_no_invalid_conditional_action::register(),
        xstate_no_invalid_transition_props::register(),
        xstate_no_misplaced_on_transition::register(),
        xstate_no_invalid_state_props::register(),
        xstate_no_ondone_outside_compound_state::register(),
        xstate_state_names::register(),
        function_inside_loop::register(),
        function_return_type::register(),
        no_while_loop::register(),
        prefer_object_has_own::register(),
        prefer_exponentiation_operator::register(),
        no_indexof_equality::register(),
        prefer_array_to_reversed::register(),
        prefer_array_to_sorted::register(),
        prefer_array_to_spliced::register(),
        prefer_array_fill::register(),
        prefer_array_from_map::register(),
        ban_dependencies::register(),
        os_command::register(),
        xpath_injection::register(),
        prefer_url_canparse::register(),
        prefer_timer_args::register(),
        no_global_types_file::register(),
        post_message_origin::register(),
        no_this_mutation::register(),
        zod_brand_ids::register(),
        zod_transform_requires_pipe::register(),
        zod_validate_env_at_startup::register(),
        zod_no_optional_and_default_together::register(),
        zod_no_unknown_schema::register(),
        zod_require_schema_suffix::register(),
        // v4.0 — 302 candidate rules (skills-rules-candidates.md)
        api_branded_id_types::register(),
        api_no_internal_ids_in_response::register(),
        api_no_nullable_variant_fields::register(),
        api_put_vs_patch::register(),
        api_route_version_prefix::register(),
        api_separate_input_output_types::register(),
        api_validate_at_boundaries::register(),
        better_auth_client_framework_import::register(),
        better_auth_drizzle_useplural::register(),
        better_auth_email_verification_handler::register(),
        better_auth_expo_no_cookie_auth::register(),
        better_auth_no_duplicate_baseurl::register(),
        better_auth_no_duplicate_secret::register(),
        better_auth_required_user_fields::register(),
        better_auth_reset_password_handler::register(),
        better_auth_secret_min_length::register(),
        better_auth_session_infer_type::register(),
        better_result_await_inside_gen::register(),
        better_result_caller_must_handle::register(),
        better_result_catch_returns_tagged::register(),
        better_result_constructor_spreads_args::register(),
        better_result_no_catch_panic::register(),
        better_result_no_manual_propagation::register(),
        better_result_no_mixed_throw::register(),
        better_result_no_nullable_return::register(),
        better_result_no_param_properties::register(),
        better_result_no_promise_catch::register(),
        better_result_no_rewrap_error::register(),
        better_result_no_throw::register(),
        better_result_no_try_catch::register(),
        better_result_prefer_map_single::register(),
        better_result_prefer_matcherror_exhaustive::register(),
        better_result_require_gen_for_chains::register(),
        better_result_tag_matches_classname::register(),
        better_result_tagged_error_cause_unknown::register(),
        better_result_tagged_error_message::register(),
        better_result_try_requires_catch::register(),
        ci_cache_key_includes_lockfile::register(),
        ci_checkout_action_pinned::register(),
        ci_docker_gha_cache::register(),
        ci_no_hardcoded_db_password::register(),
        ci_no_plaintext_secrets::register(),
        ci_playwright_report_upload::register(),
        ci_postgres_healthcheck::register(),
        ci_setup_node_cache_enabled::register(),
        ci_use_npm_ci::register(),
        comment_max_words::register(),
        css_calc_needs_spaces::register(),
        css_custom_property_needs_var::register(),
        css_font_family_needs_generic::register(),
        css_font_family_quotes::register(),
        css_keyframe_no_duplicate_selectors::register(),
        css_keyframe_no_important::register(),
        css_no_deprecated_media_type::register(),
        css_no_deprecated_property_value::register(),
        css_no_duplicate_custom_properties::register(),
        css_no_duplicate_font_family::register(),
        css_no_duplicate_properties::register(),
        css_no_empty_block::register(),
        css_no_empty_comment::register(),
        css_no_invalid_hex::register(),
        css_no_invalid_media_query::register(),
        css_no_nonstandard_gradient_direction::register(),
        css_no_redundant_longhand::register(),
        css_no_shorthand_overrides_longhand::register(),
        css_no_unknown_function::register(),
        css_no_unknown_media_feature::register(),
        css_no_unknown_media_value::register(),
        css_no_unknown_property_value::register(),
        css_no_vendor_prefix_property::register(),
        css_no_vendor_prefix_value::register(),
        css_no_vendor_prefix_selector::register(),
        css_no_vendor_prefix_at_rule::register(),
        css_no_vendor_prefix_media::register(),
        css_no_deprecated_at_rule::register(),
        css_outline_none_needs_focus::register(),
        css_no_duplicate_imports::register(),
        compose_bind_localhost_ports::register(),
        compose_cap_drop_all::register(),
        compose_depends_on_condition::register(),
        compose_healthcheck_required::register(),
        compose_no_inline_secrets::register(),
        compose_no_latest_tag::register(),
        compose_no_network_host::register(),
        compose_no_privileged::register(),
        compose_require_resource_limits::register(),
        dockerfile_absolute_workdir::register(),
        dockerfile_add_for_archive_extract::register(),
        dockerfile_apk_no_cache::register(),
        dockerfile_apt_clean_lists::register(),
        dockerfile_apt_get_y_flag::register(),
        dockerfile_apt_no_recommends::register(),
        dockerfile_copy_after_install::register(),
        dockerfile_copy_from_known_stage::register(),
        dockerfile_copy_from_not_self::register(),
        dockerfile_copy_needs_workdir::register(),
        dockerfile_copy_trailing_slash::register(),
        dockerfile_dnf_clean_all::register(),
        dockerfile_dnf_y_flag::register(),
        dockerfile_env_no_self_reference::register(),
        dockerfile_exec_form_cmd::register(),
        dockerfile_instruction_order::register(),
        dockerfile_no_add_for_files::register(),
        dockerfile_no_apt_end_user::register(),
        dockerfile_no_cd_in_run::register(),
        dockerfile_no_latest_tag::register(),
        dockerfile_no_from_platform::register(),
        dockerfile_no_maintainer::register(),
        dockerfile_no_multiple_cmd::register(),
        dockerfile_no_multiple_entrypoint::register(),
        dockerfile_no_onbuild_recursion::register(),
        dockerfile_no_secrets_in_arg::register(),
        dockerfile_no_secrets_in_copy::register(),
        dockerfile_no_secrets_in_env::register(),
        dockerfile_no_shell_utils_in_run::register(),
        dockerfile_no_sudo::register(),
        dockerfile_no_zypper_dist_upgrade::register(),
        dockerfile_pin_exact_version::register(),
        dockerfile_pip_no_cache_dir::register(),
        dockerfile_pipefail::register(),
        dockerfile_require_dockerignore::register(),
        dockerfile_require_healthcheck::register(),
        dockerfile_require_multi_stage::register(),
        dockerfile_require_non_root_user::register(),
        dockerfile_single_healthcheck::register(),
        dockerfile_unique_stage_names::register(),
        dockerfile_use_cache_mount::register(),
        dockerfile_use_frozen_lockfile::register(),
        dockerfile_use_npm_ci::register(),
        dockerfile_no_curl_and_wget::register(),
        dockerfile_useradd_low_uid::register(),
        dockerfile_wget_progress_flag::register(),
        dockerfile_label_not_empty::register(),
        dockerfile_label_url_format::register(),
        dockerfile_valid_port::register(),
        dockerfile_yarn_cache_clean::register(),
        dockerfile_yum_clean_all::register(),
        dockerfile_yum_y_flag::register(),
        dockerfile_zypper_clean::register(),
        dockerfile_zypper_non_interactive::register(),
        dockerignore_must_exclude_sensitive::register(),
        drizzle_camel_snake_column_names::register(),
        drizzle_config_satisfies::register(),
        drizzle_consistent_table_naming::register(),
        drizzle_created_at_default_now::register(),
        drizzle_json_requires_type::register(),
        drizzle_junction_composite_pk::register(),
        drizzle_multi_statement_tx::register(),
        drizzle_no_new_pool_per_request::register(),
        drizzle_pool_requires_timeouts::register(),
        drizzle_prefer_findmany_relations::register(),
        drizzle_prefer_inarray::register(),
        drizzle_prefer_infer_select::register(),
        drizzle_prepared_placeholder::register(),
        drizzle_serverless_pool_max_one::register(),
        drizzle_soft_delete_filter::register(),
        drizzle_updated_at_on_update::register(),
        drizzle_zod_omit_generated::register(),
        function_doc_banned_verbs::register(),
        i18n_key_exists::register(),
        i18n_key_requires_domain_prefix::register(),
        i18n_max_key_depth::register(),
        i18n_no_english_key::register(),
        i18n_no_manual_list_join::register(),
        i18n_use_singleton_outside_react::register(),
        k8s_deployment_anti_affinity::register(),
        k8s_disallow_privilege_escalation::register(),
        k8s_hpa_min_three_replicas::register(),
        k8s_job_ttl_required::register(),
        k8s_min_replicas_two::register(),
        k8s_no_allow_privileged_scc::register(),
        k8s_no_default_service_account::register(),
        k8s_no_deprecated_extensions_api::register(),
        k8s_no_deprecated_service_account_field::register(),
        k8s_no_docker_sock_mount::register(),
        k8s_no_duplicate_env_vars::register(),
        k8s_no_exposed_services::register(),
        k8s_no_host_ipc::register(),
        k8s_no_host_network::register(),
        k8s_no_host_pid::register(),
        k8s_no_latest_image_tag::register(),
        k8s_no_plaintext_secret_in_git::register(),
        k8s_no_privileged_container::register(),
        k8s_no_privileged_ports::register(),
        k8s_no_secret_in_env_literal::register(),
        k8s_no_secrets_in_configmap::register(),
        k8s_no_sensitive_host_mounts::register(),
        k8s_no_unsafe_proc_mount::register(),
        k8s_no_unsafe_sysctls::register(),
        k8s_no_writable_host_mount::register(),
        k8s_pdb_eviction_policy::register(),
        k8s_prefer_secret_files_over_env::register(),
        k8s_rbac_no_cluster_admin_binding::register(),
        k8s_rbac_no_create_pods::register(),
        k8s_rbac_no_secret_access::register(),
        k8s_rbac_no_wildcard_resources::register(),
        k8s_rbac_no_wildcard_verbs::register(),
        k8s_require_drop_all_caps::register(),
        k8s_require_explicit_namespace::register(),
        k8s_require_ingress_tls::register(),
        k8s_require_liveness_probe::register(),
        k8s_require_network_policy::register(),
        k8s_require_pod_disruption_budget::register(),
        k8s_require_read_only_root::register(),
        k8s_require_readiness_probe::register(),
        k8s_require_resource_limits::register(),
        k8s_require_resource_requests::register(),
        k8s_require_run_as_non_root::register(),
        k8s_require_standard_labels::register(),
        k8s_restart_policy_required::register(),
        k8s_rolling_update_zero_unavailable::register(),
        k8s_mismatching_selector::register(),
        k8s_probe_port_exists::register(),
        k8s_invalid_target_ports::register(),
        k8s_no_ssh_port::register(),
        k8s_priority_class_name::register(),
        k8s_dnsconfig_options::register(),
        k8s_dangling_hpa::register(),
        k8s_dangling_ingress::register(),
        k8s_dangling_network_policy::register(),
        k8s_dangling_network_policy_peer::register(),
        k8s_dangling_service::register(),
        k8s_dangling_service_monitor::register(),
        k8s_non_existent_service_account::register(),
        k8s_env_value_from_resolves::register(),
        no_history_in_comments::register(),
        no_shallow_passthrough_method::register(),
        perf_font_face_display_swap::register(),
        perf_font_preload_crossorigin::register(),
        perf_img_fetchpriority_high::register(),
        perf_img_modern_format::register(),
        perf_no_google_fonts_link::register(),
        perf_no_render_blocking_css::register(),
        perf_prefers_reduced_motion::register(),
        perf_route_level_code_split::register(),
        react_no_barrel_import_known_libs::register(),
        react_no_blocking_log_after_mutation::register(),
        react_no_boolean_variant_props::register(),
        react_no_chained_filter_map_reduce::register(),
        react_no_dedup_filter_indexof::register(),
        react_no_destructure_zustand_store::register(),
        react_no_find_in_map_loop::register(),
        react_no_interleaved_layout_rw::register(),
        react_no_setstate_without_updater::register(),
        react_no_sort_for_extrema::register(),
        react_no_unwrapped_localstorage::register(),
        react_no_use_client_without_client_api::register(),
        react_no_usestate_high_frequency::register(),
        react_require_content_visibility::register(),
        react_require_versioned_storage_key::register(),
        rn_auth_token_securestore::register(),
        rn_biometrics_hardware_check::register(),
        rn_expo_router_layout_required::register(),
        rn_flashlist_estimated_item_size::register(),
        rn_flashlist_over_flatlist::register(),
        rn_image_source_object::register(),
        rn_memo_list_items::register(),
        rn_no_inline_renderitem::register(),
        rn_no_inline_styles::register(),
        rn_no_react_navigation_stack::register(),
        rn_no_string_route_names::register(),
        rn_push_permissions_before_token::register(),
        rn_push_token_requires_projectid::register(),
        rn_raw_string_in_text::register(),
        rn_reanimated_over_animated::register(),
        rn_router_replace_after_login::register(),
        rust_asref_path_for_fs_fns::register(),
        rust_no_arc_mutex_tree::register(),
        rust_no_println_in_async::register(),
        rust_workspace_deps_centralized::register(),
        rust_workspace_lints_shared::register(),
        security_bcrypt_min_rounds::register(),
        security_cookie_no_samesite_none::register(),
        security_no_cors_reflect_origin::register(),
        security_no_deserialize_untrusted::register(),
        security_no_password_in_log::register(),
        security_no_query_without_ownership::register(),
        security_no_sri_missing::register(),
        security_require_helmet::register(),
        security_require_hsts::register(),
        security_require_oauth_state::register(),
        security_require_pkce_oauth::register(),
        security_require_rate_limit_auth::register(),
        shadcn_avatar_requires_fallback::register(),
        shadcn_button_icon_data_attr::register(),
        shadcn_dialog_requires_title::register(),
        shadcn_no_custom_badge::register(),
        shadcn_no_custom_skeleton::register(),
        shadcn_no_hr_use_separator::register(),
        shadcn_no_manual_dark_overrides::register(),
        shadcn_no_manual_zindex_overlays::register(),
        shadcn_no_raw_tailwind_colors::register(),
        shadcn_no_space_x_y::register(),
        shadcn_no_toggle_group_manual::register(),
        shadcn_sheet_requires_title::register(),
        shadcn_tabs_trigger_in_list::register(),
        sql_add_constraint_not_valid::register(),
        sql_boolean_column_prefix::register(),
        sql_constraint_naming_convention::register(),
        sql_fk_naming_convention::register(),
        sql_no_disable_autovacuum::register(),
        sql_no_drop_column_without_expand::register(),
        sql_no_function_on_indexed_column::register(),
        sql_no_is_deleted_boolean::register(),
        sql_no_now_in_transaction::register(),
        sql_no_rename_column::register(),
        sql_no_reserved_keyword_identifiers::register(),
        sql_no_select_then_insert_race::register(),
        sql_no_truncate_in_app::register(),
        sql_no_union_when_union_all::register(),
        sql_no_uuidv4_primary_key::register(),
        sql_require_search_path::register(),
        sql_singular_table_names::register(),
        tailwind_min_touch_target::register(),
        tailwind_no_legacy_directives::register(),
        tailwind_no_manual_dark_variants::register(),
        tailwind_no_off_scale_spacing::register(),
        tailwind_no_raw_color_utilities::register(),
        tailwind_no_tailwindcss_animate::register(),
        tailwind_no_transition_all_layout::register(),
        tailwind_require_focus_ring::register(),
        tailwind_require_motion_reduce::register(),
        tailwind_require_responsive_grid::register(),
        tailwind_require_responsive_text::register(),
        tanstack_query_dependent_needs_enabled::register(),
        tanstack_query_infinite_initial_page_param::register(),
        tanstack_query_invalidate_after_mutation::register(),
        tanstack_query_max_pages_requires_both::register(),
        tanstack_query_no_enabled_on_suspense::register(),
        tanstack_query_no_global_onerror_v5::register(),
        tanstack_query_no_v4_import_path::register(),
        tanstack_query_object_syntax::register(),
        tanstack_query_pass_signal_to_fetch::register(),
        tanstack_query_serializable_key::register(),
        tanstack_query_test_retry_false::register(),
        tanstack_start_api_route_json_helper::register(),
        tanstack_start_no_date_now_in_render::register(),
        tanstack_start_no_fetch_to_own_api::register(),
        tanstack_start_no_window_in_render::register(),
        tanstack_start_route_protection_beforeload::register(),
        tanstack_start_server_fn_post_for_mutations::register(),
        tanstack_start_server_fn_use_notfound::register(),
        tanstack_start_session_cookie_httponly::register(),
        tanstack_start_session_cookie_samesite::register(),
        tanstack_start_session_cookie_secure::register(),
        tanstack_start_session_secret_min_length::register(),
        testing_no_concurrent_without_context_expect::register(),
        testing_no_conditional_assertion::register(),
        testing_no_mocking_internal_modules::register(),
        testing_no_mocktimers_without_restore::register(),
        testing_no_shared_state::register(),
        testing_no_stubglobal_without_restore::register(),
        testing_no_try_catch_swallow::register(),
        testing_require_testid_kebab_case::register(),
        ts_assertion_fn_must_be_declaration::register(),
        ts_bounded_recursive_generic::register(),
        ts_branded_type_no_direct_cast::register(),
        ts_declare_global_requires_export::register(),
        ts_no_as_narrowing::register(),
        ts_no_generic_return_only::register(),
        ts_no_large_string_union::register(),
        ts_no_mixed_decorator_systems::register(),
        ts_no_mixed_sync_async_returns::register(),
        ts_no_narrowing_across_closures::register(),
        ts_no_unused_generic_parameter::register(),
        ts_overload_signature_order::register(),
        ts_prefer_interface_extends::register(),
        ts_require_variance_annotation::register(),
        ui_animate_presence_requires_exit::register(),
        ui_animate_transform_opacity_only::register(),
        ui_antialiased_on_root::register(),
        ui_concentric_border_radius::register(),
        ui_exit_duration_shorter_enter::register(),
        ui_hover_gated_media_query::register(),
        ui_min_hit_area_44::register(),
        ui_no_display_none_exit::register(),
        ui_no_keyframes_for_interruptible::register(),
        ui_no_pure_black::register(),
        ui_no_scroll_trigger_markers_prod::register(),
        ui_no_transition_all::register(),
        ui_prefers_reduced_motion::register(),
        ui_stagger_children_cap::register(),
        ui_symmetric_initial_exit::register(),
        ui_tabular_nums_on_data::register(),
        ui_text_balance_headings::register(),
        vue_computed_no_side_effects::register(),
        vue_custom_directive_v_prefix::register(),
        vue_define_model_over_modelvalue::register(),
        vue_inject_key_typed::register(),
        vue_no_filter_sort_in_template::register(),
        vue_no_ssr_globals_in_setup::register(),
        vue_no_usestore_top_level::register(),
        vue_no_v_if_with_v_for::register(),
        vue_no_value_on_reactive::register(),
        vue_no_watch_reactive_property::register(),
        vue_ref_value_in_script::register(),
        vue_scoped_styles_preferred::register(),
        vue_setup_store_return_all::register(),
        vue_shallowref_for_primitives::register(),
        vue_typed_define_props_emits::register(),
        vue_use_template_ref::register(),
        vue_v_memo_requires_v_for::register(),
        vue_watch_immediate_over_onmounted::register(),
        vue_withdefaults_factory::register(),
        vue_button_has_type::register(),
        vue_checked_requires_onchange::register(),
        vue_component_pascal_case::register(),
        vue_iframe_missing_sandbox::register(),
        vue_no_adjacent_inline_elements::register(),
        vue_no_array_index_key::register(),
        vue_no_comment_textnodes::register(),
        vue_no_duplicate_props::register(),
        vue_no_invalid_html_attribute::register(),
        vue_no_namespace::register(),
        vue_no_script_url::register(),
        vue_no_target_blank::register(),
        vue_no_unescaped_entities::register(),
        vue_props_no_spread_multi::register(),
        vue_self_closing_comp::register(),
        vue_void_elements_no_children::register(),
        zod_no_coerce_on_financial::register(),
        zod_no_manual_types::register(),
        zod_no_schema_in_hot_path::register(),
        zod_prefer_extend_over_merge::register(),
        zod_prefer_loose_object::register(),
        zod_prefer_overwrite_v4::register(),
        zod_prefer_strict_object::register(),
        zod_prefer_stringbool::register(),
        zod_record_two_args::register(),
        zod_require_input_for_transforms::register(),
        zod_require_multipleof_currency::register(),

        // --- batch-2 new rules ---
        a11y_button_without_accessible_name::register(),
        a11y_dialog_missing_aria_labelledby::register(),
        api_no_status_in_body::register(),
        api_no_unbounded_input_field::register(),
        api_response_envelope_consistency::register(),
        drizzle_decimal_for_money::register(),
        drizzle_dollar_type_widens_unknown::register(),
        drizzle_findfirst_without_where::register(),
        drizzle_leftjoin_nullable_handling::register(),
        drizzle_migrations_no_data_in_schema_migration::register(),
        drizzle_no_db_query_in_loop::register(),
        drizzle_no_drizzle_kit_in_runtime::register(),
        drizzle_prefer_select_columns::register(),
        drizzle_relations_missing_inverse::register(),
        elysia_aot_dynamic_route::register(),
        elysia_decorate_uses_request_data::register(),
        elysia_derive_async_no_await::register(),
        elysia_guard_overrides_route_schema::register(),
        elysia_model_reference_by_string::register(),
        elysia_onerror_before_plugin::register(),
        elysia_response_t_unknown::register(),
        elysia_set_status_after_return::register(),
        elysia_t_unknown_format_string::register(),
        elysia_ws_message_no_schema::register(),
        next_no_server_action_without_use_server::register(),
        react_no_async_useeffect_callback::register(),
        react_no_form_action_async_no_pending::register(),
        react_no_ref_read_during_render::register(),
        react_no_router_push_in_effect_no_dep::register(),
        react_no_setstate_no_cancel_flag::register(),
        react_no_state_setter_in_render::register(),
        react_no_sync_layout_effect_in_ssr::register(),
        react_no_useeffect_event_in_deps::register(),
        react_prefer_use_action_state::register(),
        react_prefer_use_optimistic::register(),
        react_use_no_conditional::register(),
        rust_assert_eq_with_bool_literal::register(),
        rust_box_dyn_error_without_send_sync::register(),
        rust_clone_in_iter_chain::register(),
        rust_collect_then_into_iter::register(),
        rust_const_for_static_no_interior_mut::register(),
        rust_drop_calls_self_lock::register(),
        rust_env_var_unwrap_at_runtime::register(),
        rust_eprintln_in_library::register(),
        rust_float_eq_partial_cmp::register(),
        rust_format_args_in_log_macro::register(),
        rust_hash_partial_eq_mismatch::register(),
        rust_if_let_mutex_lock::register(),
        rust_iter_count_vs_len::register(),
        rust_loop_collect_into_existing_vec::register(),
        rust_match_lock_guard_scrutinee::register(),
        rust_ord_partial_ord_inconsistent::register(),
        rust_panic_in_drop::register(),
        rust_partial_eq_without_eq::register(),
        rust_repeated_string_concat_in_loop::register(),
        rust_select_without_biased::register(),
        rust_send_sync_unsafe_impl_on_pointer_field::register(),
        rust_serde_untagged_without_explicit_default::register(),
        rust_string_push_str_format::register(),
        rust_thread_spawn_in_async::register(),
        rust_to_string_in_format_arg::register(),
        sql_add_not_null_without_default::register(),
        sql_alter_column_type_unsafe::register(),
        sql_check_constraint_no_volatile_function::register(),
        sql_create_index_in_transaction::register(),
        sql_drop_table_no_cascade_warning::register(),
        sql_index_on_low_cardinality_boolean::register(),
        sql_jsonb_not_json::register(),
        sql_no_for_update_without_skip_locked::register(),
        sql_no_serial_use_identity::register(),
        sql_pg_enum_with_alter_type_add_value::register(),
        sql_recursive_cte_no_termination::register(),
        sql_text_search_missing_language::register(),
        tailwind_no_negative_z_index_on_interactive::register(),
        tailwind_no_overflow_hidden_on_focus_container::register(),
        tailwind_no_text_size_below_12px::register(),
        tailwind_no_w_screen_h_screen_on_mobile::register(),
        tailwind_require_aspect_ratio_on_media::register(),
        tanstack_query_dehydrate_no_pending_in_ssr::register(),
        tanstack_query_no_async_query_fn_without_await::register(),
        tanstack_query_no_query_in_render_loop::register(),
        tanstack_query_no_set_query_data_without_key_factory::register(),
        tanstack_query_select_must_be_stable::register(),
        tanstack_router_search_no_use_state_for_url_state::register(),
        ts_no_assert_never_in_default::register(),
        ts_no_enum_object_literal_pattern::register(),
        ts_no_floating_promise_in_array_method::register(),
        ts_no_object_keys_typed_loop::register(),
        ts_no_promise_void_function_misuse::register(),
        ts_no_redundant_async::register(),
        ts_no_void_returning_assigned::register(),
        ts_prefer_promise_with_resolvers::register(),
        zod_no_parse_in_render::register(),
        zod_no_passthrough_on_api_input::register(),
        zod_no_safeparse_without_check::register(),
    ];
    rules.extend(delegated::register_all());
    rules.extend(delegated::register_tsgolint());
    rules
}
