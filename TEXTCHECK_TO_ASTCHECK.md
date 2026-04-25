# TextCheck to AstCheck Conversion Plan

93 TEXT-ONLY rules targeting languages with tree-sitter grammars.

## Convertible to AstCheck (32 rules — code structure analysis)

These rules scan TS/TSX/JS source for specific patterns. AST is more robust
(avoids matching inside comments/strings).

### TanStack Query (10)
- [ ] tanstack_query_fn_must_throw_on_error
- [ ] tanstack_query_no_cache_time
- [ ] tanstack_query_no_enabled_true
- [ ] tanstack_query_no_is_loading
- [ ] tanstack_query_no_keep_previous_data_prop
- [ ] tanstack_query_no_query_callbacks
- [ ] tanstack_query_no_use_error_boundary
- [ ] tanstack_query_prefer_key_factory
- [ ] tanstack_query_prefer_query_options
- [ ] tanstack_query_require_stale_time

### TanStack Start (6)
- [ ] tanstack_start_loader_stale_time
- [ ] tanstack_start_no_client_import_in_server_fn
- [ ] tanstack_start_require_validate_search
- [ ] tanstack_start_server_fn_file_convention
- [ ] tanstack_start_server_fn_requires_auth
- [ ] tanstack_start_server_fn_requires_validation

### Zod (7)
- [ ] zod_no_optional_nullable_chain
- [ ] zod_prefer_discriminated_union
- [ ] zod_prefer_safe_parse
- [ ] zod_refine_requires_path
- [ ] zod_require_error_messages
- [ ] zod_string_min_1_required
- [ ] zod_trim_before_min

### React (3)
- [ ] react_prefer_use_transition
- [ ] react_server_action_requires_auth
- [ ] react_server_action_requires_validation

### API (2)
- [ ] api_list_requires_pagination
- [ ] api_no_array_root_response

### Drizzle (2)
- [ ] drizzle_no_sql_raw_with_variable
- [ ] drizzle_zod_prefer_generated_schema

### i18n (1)
- [ ] i18n_no_string_concat_with_translation

### SQL TS-side (1)
- [ ] sql_require_transaction_timeout

## Convertible to AstCheck (12 rules — Tailwind class scanning)

These scan className/class attributes for Tailwind patterns. AST finds the
actual JSX attribute values instead of text-matching.

- [ ] tailwind_classnames_order
- [ ] tailwind_enforces_negative_arbitrary_values
- [ ] tailwind_no_apply_for_variants
- [ ] tailwind_no_arbitrary_z_index
- [ ] tailwind_no_conflicting_classes
- [ ] tailwind_no_deprecated_classes
- [ ] tailwind_no_duplicate_classes
- [ ] tailwind_no_important_modifier
- [ ] tailwind_no_magic_spacing
- [ ] tailwind_no_unnecessary_whitespace
- [ ] tailwind_prefer_shorthand
- [ ] tailwind_prefer_size_shorthand

## Convertible to AstCheck (18 rules — JSDoc comment validation)

These parse JSDoc comment blocks. AST can find `comment` nodes directly.

- [ ] jsdoc_check_property_names
- [ ] jsdoc_check_tag_names
- [ ] jsdoc_check_template_names
- [ ] jsdoc_check_types
- [ ] jsdoc_check_values
- [ ] jsdoc_require_hyphen_before_param_description
- [ ] jsdoc_require_next_description
- [ ] jsdoc_require_param_description
- [ ] jsdoc_require_param_name
- [ ] jsdoc_require_property
- [ ] jsdoc_require_property_description
- [ ] jsdoc_require_property_name
- [ ] jsdoc_require_rejects
- [ ] jsdoc_require_returns_description
- [ ] jsdoc_require_tags
- [ ] jsdoc_require_template
- [ ] jsdoc_require_template_description
- [ ] jsdoc_require_yields
- [ ] jsdoc_require_yields_check
- [ ] jsdoc_require_yields_description
- [ ] jsdoc_valid_types

## Convertible to AstCheck (5 rules — comment scanning)

- [ ] banned_comment_words
- [ ] comment_prose_quality
- [ ] no_section_divider_comments
- [ ] no_abusive_eslint_disable
- [ ] expiring_todo_comments

## Convertible to AstCheck (8 rules — SQL in TS/JS strings)

Same pattern as existing SQL AstCheck rules: find string literals containing
SQL, check the SQL content. Language::Sql stays TextCheck.

- [ ] sql_advisory_lock_prefer_xact
- [ ] sql_create_index_concurrently
- [ ] sql_no_float_for_money
- [ ] sql_no_like_wildcard_prefix
- [ ] sql_no_pg_enum
- [ ] sql_no_select_star
- [ ] sql_nullable_requires_comment
- [ ] sql_prefer_exists_over_in

## Convertible to AstCheck (1 rule — CSS)

- [ ] i18n_prefer_logical_css_properties

## NOT convertible — must stay TextCheck (16 rules)

- filename_naming_convention — path-only, no content
- folder_naming_convention — path-only, no content
- no_index_file — path-only
- no_common_grab_bag — path-only
- no_empty_file — trivial content check
- no_bidi_characters — raw byte scanning
- no_hardcoded_ip — regex across all text incl. comments
- no_hardcoded_secret — regex across all text
- no_hardcoded_secret_signature — regex
- migration_needs_lock_timeout — SQL text pattern
- migration_needs_rollback — SQL text pattern
- package_json_sorted_deps — JSON structure, no grammar
- package_json_unique_deps — JSON structure, no grammar
- dockerignore_must_exclude_sensitive — plain text, no grammar
