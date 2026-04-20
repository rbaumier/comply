# SonarJS rules candidates for comply

Rules already implemented or covered by oxlint/tsc are excluded.
Grouped by category. Prioritize within each group.

## bugs (35)

- [ ] arguments-order — Parameters should be passed in the correct order
- [ ] array-callback-without-return — Callbacks of array methods should have return statements
- [ ] comma-or-logical-or-case — Comma and logical OR operators should not be used in switch cases
- [ ] for-loop-increment-sign — A "for" loop update clause should move the counter in the right direction
- [ ] generator-without-yield — Generators should explicitly "yield" a value
- [ ] inconsistent-function-call — Functions should be called consistently with or without "new"
- [ ] index-of-compare-to-positive-number — "indexOf" checks should not be for positive numbers
- [ ] misplaced-loop-counter — "for" loop increment clauses should modify the loops' counters
- [ ] no-array-delete — "delete" should not be used on arrays
- [ ] no-associative-arrays — Array indexes should be numeric
- [ ] no-built-in-override — Built-in objects should not be overridden
- [ ] no-case-label-in-switch — "switch" statements should not contain non-case labels
- [ ] no-collection-size-mischeck — Collection size and array length comparisons should make sense
- [ ] no-duplicated-branches — Two branches in a conditional structure should not have exactly the same implementation
- [ ] no-element-overwrite — Collection elements should not be replaced unconditionally
- [ ] no-equals-in-for-termination — Equality operators should not be used in "for" loop termination conditions
- [ ] no-for-in-iterable — "for in" should not be used with iterables
- [ ] no-function-declaration-in-block — Function declarations should not be made within blocks
- [ ] no-identical-conditions — "if/else if" chains and "switch" cases should not have the same condition
- [ ] no-identical-expressions — Identical expressions should not be used on both sides of a binary operator
- [ ] no-ignored-return — Return values from functions without side effects should not be ignored
- [ ] no-in-misuse — "in" should not be used on arrays
- [ ] no-inconsistent-returns — Functions should use "return" consistently
- [ ] no-incorrect-string-concat — Strings and non-strings should not be added
- [ ] no-invariant-returns — Function returns should not be invariant
- [ ] no-misleading-array-reverse — Array-mutating methods should not be used misleadingly
- [ ] no-nested-assignment — Assignments should not be made from within sub-expressions
- [ ] no-nested-incdec — Increment (++) and decrement (--) operators should not be used in a method call or mixed with other 
- [ ] no-primitive-wrappers — Wrapper objects should not be used for primitive types
- [ ] no-unthrown-error — Errors should not be created without being thrown
- [ ] no-useless-increment — Values should not be uselessly incremented
- [ ] non-existent-operator — Non-existent operators '=+', '=-' and '=!' should not be used
- [ ] operation-returning-nan — Arithmetic operations should not result in "NaN"
- [ ] strings-comparison — Comparison operators should not be used with strings
- [ ] useless-string-operation — Results of operations on strings should not be ignored

## security (17)

- [ ] confidential-information-logging — Allowing confidential information to be logged is security-sensitive
- [ ] dynamically-constructed-templates — Templates should not be constructed dynamically
- [ ] hardcoded-secret-signatures — Credentials should not be hard-coded
- [ ] hashing — Using weak hashing algorithms is security-sensitive
- [ ] insecure-jwt-token — JWT should be signed and verified with strong cipher algorithms
- [ ] no-os-command-from-path — Searching OS commands in PATH is security-sensitive
- [ ] no-weak-cipher — Cipher algorithms should be robust
- [ ] no-weak-keys — Cryptographic keys should be robust
- [ ] os-command — Using shell interpreter when executing OS commands is security-sensitive
- [ ] post-message — Origins should be verified during cross-origin communications
- [ ] pseudo-random — Using pseudorandom number generators (PRNGs) is security-sensitive
- [ ] sql-queries — Formatting SQL queries is security-sensitive
- [ ] unverified-certificate — Server certificates should be verified during SSL/TLS connections
- [ ] unverified-hostname — Server hostnames should be verified during SSL/TLS connections
- [ ] weak-ssl — Weak SSL/TLS protocols should not be used
- [ ] xml-parser-xxe — XML parsers should not be vulnerable to XXE attacks
- [ ] xpath — Executing XPath expressions is security-sensitive

## code-quality (35)

- [ ] arguments-usage — "arguments" should not be accessed directly
- [ ] array-constructor — Array constructors should not be used
- [ ] bitwise-operators — Bitwise operators should not be used in boolean contexts
- [ ] bool-param-default — Optional boolean parameters should have default value
- [ ] constructor-for-side-effects — Objects should not be created to be dropped immediately without being used
- [ ] cyclomatic-complexity — Cyclomatic Complexity of functions should not be too high
- [ ] destructuring-assignment-syntax — Destructuring syntax should be used for assignments
- [ ] elseif-without-else — "if ... else if" constructs should end with "else" clauses
- [ ] expression-complexity — Expressions should not be too complex
- [ ] file-name-differ-from-class — Default export names and file names should match
- [ ] function-inside-loop — Functions should not be defined inside loops
- [ ] function-return-type — Functions should always return the same type
- [ ] nested-control-flow — Control flow statements "if", "for", "while", "switch" and "try" should not be nested too deeply
- [ ] no-async-constructor — Constructors should not contain asynchronous operations
- [ ] no-duplicate-string — String literals should not be duplicated
- [ ] no-ignored-exceptions — Exceptions should not be ignored
- [ ] no-inverted-boolean-check — Boolean checks should not be inverted
- [ ] no-nested-functions — Functions should not be nested too deeply
- [ ] no-nested-switch — "switch" statements should not be nested
- [ ] no-redundant-jump — Jump statements should not be redundant
- [ ] no-selector-parameter — Methods should not contain selector parameters
- [ ] no-small-switch — "if" statements should be preferred over "switch" when simpler
- [ ] no-try-promise — Promise rejections should not be caught by "try" blocks
- [ ] no-undefined-argument — "undefined" should not be passed as the value of optional parameters
- [ ] no-undefined-assignment — "undefined" should not be assigned
- [ ] no-unenclosed-multiline-block — Multiline blocks should be enclosed in curly braces
- [ ] no-unused-collection — Collection contents should be used
- [ ] no-unused-function-argument — Unused function parameters should be removed
- [ ] prefer-default-last — "default" clauses should be last
- [ ] prefer-object-literal — Object literal syntax should be used
- [ ] prefer-promise-shorthand — Shorthand promises should be used
- [ ] prefer-regexp-exec — "RegExp.exec()" should be preferred over "String.match()"
- [ ] prefer-while — A "while" loop should be used instead of a "for" loop
- [ ] reduce-initial-value — "Array.reduce()" calls should include an initial value
- [ ] too-many-break-or-continue-in-loop — Loops should not contain more than a single "break" or "continue" statement

## testing (5)

- [ ] assertions-in-tests — Tests should include assertions
- [ ] inverted-assertion-arguments — Assertion arguments should be passed in the correct order
- [ ] no-incomplete-assertions — Assertions should be complete
- [ ] no-same-argument-assert — Assertions should not be given twice the same argument
- [ ] test-check-exception — Tests should check which exception is thrown

## react (5)

- [ ] jsx-no-leaked-render — React components should not render non-boolean condition values
- [ ] no-hook-setter-in-body — React's useState hook should not be used directly in the render function or body of a component
- [ ] no-uniq-key — JSX list components keys should match up between renders
- [ ] no-useless-react-setstate — React state setter function should not be called with its matching state variable

## typescript (9)

- [ ] max-union-size — Union types should not have too many elements
- [ ] no-duplicate-in-composite — Union and intersection types should not include duplicated constituents
- [ ] no-redundant-optional — Optional property declarations should not use both '?' and 'undefined' syntax
- [ ] no-return-type-any — Primitive return types should be used
- [ ] no-useless-intersection — Type intersections should use meaningful types
- [ ] prefer-type-guard — Type predicates should be used
- [ ] public-static-readonly — Public "static" fields should be read-only
- [ ] redundant-type-aliases — Redundant type aliases should not be used
- [ ] use-type-alias — Type aliases should be used

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
