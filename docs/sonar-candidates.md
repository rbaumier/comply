# SonarJS rules candidates for comply

Rules already implemented or covered by oxlint/tsc are excluded.
Grouped by category. Prioritize within each group.

## bugs (35)

- [x] arguments-order — Parameters should be passed in the correct order → `arguments_order`
- [x] array-callback-without-return — Callbacks of array methods should have return statements → `array_callback_without_return`
- [x] comma-or-logical-or-case — Comma and logical OR operators should not be used in switch cases → `comma_or_logical_or_case`
- [x] for-loop-increment-sign — A "for" loop update clause should move the counter in the right direction → `for_loop_increment_sign`
- [x] generator-without-yield — Generators should explicitly "yield" a value → `generator_without_yield`
- [x] inconsistent-function-call — Functions should be called consistently with or without "new" → `inconsistent_function_call`
- [x] index-of-compare-to-positive-number — "indexOf" checks should not be for positive numbers → `index_of_compare_to_positive`
- [x] misplaced-loop-counter — "for" loop increment clauses should modify the loops' counters → `no_misplaced_loop_counter`
- [x] no-array-delete — "delete" should not be used on arrays → `no_array_delete`
- [x] no-associative-arrays — Array indexes should be numeric → `no_associative_arrays`
- [x] no-built-in-override — Built-in objects should not be overridden → `no_built_in_override`
- [x] no-case-label-in-switch — "switch" statements should not contain non-case labels → `no_case_label_in_switch`
- [x] no-collection-size-mischeck — Collection size and array length comparisons should make sense → `no_collection_size_mischeck`
- [x] no-duplicated-branches — Two branches in a conditional structure should not have exactly the same implementation → `no_duplicated_branches`
- [x] no-element-overwrite — Collection elements should not be replaced unconditionally → `no_element_overwrite`
- [x] no-equals-in-for-termination — Equality operators should not be used in "for" loop termination conditions → `no_equals_in_for_termination`
- [x] no-for-in-iterable — "for in" should not be used with iterables → `no_for_in_iterable`
- [x] no-function-declaration-in-block — Function declarations should not be made within blocks → `no_function_declaration_in_block`
- [x] no-identical-conditions — "if/else if" chains and "switch" cases should not have the same condition → `no_identical_conditions`
- [x] no-identical-expressions — Identical expressions should not be used on both sides of a binary operator → `no_identical_expressions`
- [x] no-ignored-return — Return values from functions without side effects should not be ignored → `no_ignored_return`
- [x] no-in-misuse — "in" should not be used on arrays → `no_in_misuse`
- [x] no-inconsistent-returns — Functions should use "return" consistently → `no_inconsistent_returns`
- [x] no-incorrect-string-concat — Strings and non-strings should not be added → `no_incorrect_string_concat`
- [x] no-invariant-returns — Function returns should not be invariant → `no_invariant_returns`
- [x] no-misleading-array-reverse — Array-mutating methods should not be used misleadingly → `no_misleading_array_reverse`
- [x] no-nested-assignment — Assignments should not be made from within sub-expressions → `no_nested_assignment`
- [x] no-nested-incdec — Increment (++) and decrement (--) operators should not be used in a method call or mixed with other → `no_nested_incdec`
- [x] no-primitive-wrappers — Wrapper objects should not be used for primitive types → `no_primitive_wrappers`
- [x] no-unthrown-error — Errors should not be created without being thrown → `no_unthrown_error`
- [x] no-useless-increment — Values should not be uselessly incremented → `no_useless_increment`
- [x] non-existent-operator — Non-existent operators '=+', '=-' and '=!' should not be used → `non_existent_operator`
- [x] operation-returning-nan — Arithmetic operations should not result in "NaN" → `operation_returning_nan`
- [x] strings-comparison — Comparison operators should not be used with strings → `strings_comparison`
- [x] useless-string-operation — Results of operations on strings should not be ignored → `useless_string_operation`

## security (17)

- [x] confidential-information-logging — Allowing confidential information to be logged is security-sensitive → `no_confidential_logging`
- [x] dynamically-constructed-templates — Templates should not be constructed dynamically → `no_dynamic_template`
- [x] hardcoded-secret-signatures — Credentials should not be hard-coded → `no_hardcoded_secret_signature`
- [x] hashing — Using weak hashing algorithms is security-sensitive → `no_weak_hashing`
- [x] insecure-jwt-token — JWT should be signed and verified with strong cipher algorithms → `no_insecure_jwt`
- [ ] no-os-command-from-path — Searching OS commands in PATH is security-sensitive
- [x] no-weak-cipher — Cipher algorithms should be robust → `no_weak_cipher`
- [x] no-weak-keys — Cryptographic keys should be robust → `no_weak_keys`
- [ ] os-command — Using shell interpreter when executing OS commands is security-sensitive
- [ ] post-message — Origins should be verified during cross-origin communications
- [x] pseudo-random — Using pseudorandom number generators (PRNGs) is security-sensitive → `no_pseudo_random`
- [x] sql-queries — Formatting SQL queries is security-sensitive → `db_no_string_concat_sql`
- [x] unverified-certificate — Server certificates should be verified during SSL/TLS connections → `no_unverified_certificate`
- [x] unverified-hostname — Server hostnames should be verified during SSL/TLS connections → `no_unverified_hostname`
- [x] weak-ssl — Weak SSL/TLS protocols should not be used → `no_weak_ssl`
- [x] xml-parser-xxe — XML parsers should not be vulnerable to XXE attacks → `no_xml_external_entity`
- [ ] xpath — Executing XPath expressions is security-sensitive

## code-quality (35)

- [x] arguments-usage — "arguments" should not be accessed directly → `no_arguments_usage`
- [x] array-constructor — Array constructors should not be used → `no_array_constructor`
- [x] bitwise-operators — Bitwise operators should not be used in boolean contexts → `no_bitwise_in_boolean`
- [x] bool-param-default — Optional boolean parameters should have default value → `bool_param_default`
- [x] constructor-for-side-effects — Objects should not be created to be dropped immediately without being used → `no_constructor_side_effects`
- [x] cyclomatic-complexity — Cyclomatic Complexity of functions should not be too high → `cyclomatic_complexity`
- [x] destructuring-assignment-syntax — Destructuring syntax should be used for assignments → `prefer_destructuring_assignment`
- [x] elseif-without-else — "if ... else if" constructs should end with "else" clauses → `elseif_without_else`
- [x] expression-complexity — Expressions should not be too complex → `expression_complexity`
- [x] file-name-differ-from-class — Default export names and file names should match → `file_name_differ_from_class`
- [ ] function-inside-loop — Functions should not be defined inside loops
- [ ] function-return-type — Functions should always return the same type
- [x] nested-control-flow — Control flow statements "if", "for", "while", "switch" and "try" should not be nested too deeply → `nested_control_flow`
- [x] no-async-constructor — Constructors should not contain asynchronous operations → `no_async_constructor`
- [x] no-duplicate-string — String literals should not be duplicated → `no_duplicate_string`
- [x] no-ignored-exceptions — Exceptions should not be ignored → `no_ignored_exceptions`
- [x] no-inverted-boolean-check — Boolean checks should not be inverted → `no_inverted_boolean_check`
- [x] no-nested-functions — Functions should not be nested too deeply → `no_nested_functions`
- [x] no-nested-switch — "switch" statements should not be nested → `no_nested_switch`
- [x] no-redundant-jump — Jump statements should not be redundant → `no_redundant_jump`
- [ ] no-selector-parameter — Methods should not contain selector parameters
- [x] no-small-switch — "if" statements should be preferred over "switch" when simpler → `no_small_switch`
- [x] no-try-promise — Promise rejections should not be caught by "try" blocks → `no_try_promise`
- [x] no-undefined-argument — "undefined" should not be passed as the value of optional parameters → `no_undefined_argument`
- [x] no-undefined-assignment — "undefined" should not be assigned → `no_undefined_assignment`
- [x] no-unenclosed-multiline-block — Multiline blocks should be enclosed in curly braces → `no_unenclosed_multiline_block`
- [x] no-unused-collection — Collection contents should be used → `no_unused_collection`
- [ ] no-unused-function-argument — Unused function parameters should be removed
- [x] prefer-default-last — "default" clauses should be last → `prefer_default_last`
- [x] prefer-object-literal — Object literal syntax should be used → `prefer_object_literal`
- [x] prefer-promise-shorthand — Shorthand promises should be used → `prefer_promise_shorthand`
- [x] prefer-regexp-exec — "RegExp.exec()" should be preferred over "String.match()" → `prefer_regexp_exec`
- [x] prefer-while — A "while" loop should be used instead of a "for" loop → `prefer_while`
- [x] reduce-initial-value — "Array.reduce()" calls should include an initial value → `reduce_initial_value`
- [x] too-many-break-or-continue-in-loop — Loops should not contain more than a single "break" or "continue" statement → `too_many_break_or_continue`

## testing (5)

- [x] assertions-in-tests — Tests should include assertions → `assertions_in_tests`
- [x] inverted-assertion-arguments — Assertion arguments should be passed in the correct order → `inverted_assertion_arguments`
- [x] no-incomplete-assertions — Assertions should be complete → `no_incomplete_assertions`
- [x] no-same-argument-assert — Assertions should not be given twice the same argument → `no_same_argument_assert`
- [x] test-check-exception — Tests should check which exception is thrown → `test_check_exception`

## react (5)

- [x] jsx-no-leaked-render — React components should not render non-boolean condition values → `jsx_no_leaked_render`
- [x] no-hook-setter-in-body — React's useState hook should not be used directly in the render function or body of a component → `no_hook_setter_in_body`
- [x] no-uniq-key — JSX list components keys should match up between renders → `no_uniq_key`
- [x] no-useless-react-setstate — React state setter function should not be called with its matching state variable → `no_useless_react_setstate`

## typescript (9)

- [x] max-union-size — Union types should not have too many elements → `max_union_size`
- [x] no-duplicate-in-composite — Union and intersection types should not include duplicated constituents → `no_duplicate_in_composite`
- [x] no-redundant-optional — Optional property declarations should not use both '?' and 'undefined' syntax → `no_redundant_optional`
- [x] no-return-type-any — Primitive return types should be used → `no_return_type_any`
- [x] no-useless-intersection — Type intersections should use meaningful types → `no_useless_intersection`
- [x] prefer-type-guard — Type predicates should be used → `prefer_type_guard`
- [x] public-static-readonly — Public "static" fields should be read-only → `public_static_readonly`
- [x] redundant-type-aliases — Redundant type aliases should not be used → `redundant_type_aliases`
- [x] use-type-alias — Type aliases should be used → `use_type_alias`

## Summary

| Category | Count |
|----------|-------|
| bugs | 35 |
| security | 17 |
| code-quality | 35 |
| testing | 5 |
| react | 5 |
| typescript | 9 |
| **Total** | **106** |

Status: 100 / 106 implemented. Remaining: `no-os-command-from-path`, `os-command`, `post-message`, `xpath` (security); `function-inside-loop`, `function-return-type`, `no-selector-parameter`, `no-unused-function-argument` (code-quality).
