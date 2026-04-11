# Sonar rules — remaining 164

Rules not yet implemented in comply. Grouped by reason.

## AWS / Infrastructure (19)

- **aws-apigateway-public-api** — Creating public APIs is security-sensitive
- **aws-ec2-rds-dms-public** — Allowing public network access to cloud resources is security-sensitive
- **aws-ec2-unencrypted-ebs-volume** — Using unencrypted EBS volumes is security-sensitive
- **aws-efs-unencrypted** — Using unencrypted EFS file systems is security-sensitive
- **aws-iam-all-privileges** — Policies granting all privileges are security-sensitive
- **aws-iam-all-resources-accessible** — Policies granting access to all resources of an account are security-sensitive
- **aws-iam-privilege-escalation** — AWS IAM policies should limit the scope of permissions given
- **aws-iam-public-access** — Policies authorizing public access to resources are security-sensitive
- **aws-opensearchservice-domain** — Using unencrypted Opensearch domains is security-sensitive
- **aws-rds-unencrypted-databases** — Using unencrypted RDS DB resources is security-sensitive
- **aws-restricted-ip-admin-access** — Administration services access should be restricted to specific IP addresses
- **aws-s3-bucket-granted-access** — Granting access to S3 buckets to all or authenticated users is security-sensitive
- **aws-s3-bucket-insecure-http** — Authorizing HTTP communications with S3 buckets is security-sensitive
- **aws-s3-bucket-public-access** — Allowing public ACLs or policies on a S3 bucket is security-sensitive
- **aws-s3-bucket-server-encryption** — Disabling server-side encryption of S3 buckets is security-sensitive
- **aws-s3-bucket-versioning** — Disabling versioning of S3 buckets is security-sensitive
- **aws-sagemaker-unencrypted-notebook** — Using unencrypted SageMaker notebook instances is security-sensitive
- **aws-sns-unencrypted-topics** — Using unencrypted SNS topics is security-sensitive
- **aws-sqs-unencrypted-queue** — Using unencrypted SQS queues is security-sensitive

## Browser Security (runtime context needed) (30)

- **content-length** — Allowing requests with excessive content length is security-sensitive
- **content-security-policy** — Disabling content security policy fetch directives is security-sensitive
- **cookie-no-httponly** — Creating cookies without the "HttpOnly" flag is security-sensitive
- **cookies** — Writing cookies is security-sensitive
- **cors** — Having a permissive Cross-Origin Resource Sharing policy is security-sensitive
- **csrf** — Disabling CSRF protections is security-sensitive
- **disabled-auto-escaping** — Disabling auto-escaping in template engines is security-sensitive
- **disabled-resource-integrity** — Using remote artifacts without integrity checks is security-sensitive
- **dns-prefetching** — Allowing browsers to perform DNS prefetching is security-sensitive
- **file-permissions** — File permissions should not be set to world-accessible values
- **file-uploads** — File uploads should be restricted
- **frame-ancestors** — Disabling content security policy frame-ancestors directive is security-sensitive
- **hidden-files** — Statically serving hidden files is security-sensitive
- **insecure-cookie** — Creating cookies without the "secure" flag is security-sensitive
- **link-with-target-blank** — Authorizing an opened window to access back to the originating window is security-sensitive
- **no-intrusive-permissions** — Using intrusive permissions is security-sensitive
- **no-ip-forward** — Forwarding client IP address is security-sensitive
- **no-mime-sniff** — Allowing browsers to sniff MIME types is security-sensitive
- **no-mixed-content** — Allowing mixed-content is security-sensitive
- **no-referrer-policy** — Disabling strict HTTP no-referrer policy is security-sensitive
- **no-session-cookies-on-static-assets** — Static Assets should not serve session cookies
- **no-unsafe-unzip** — Expanding archive files without controlling resource consumption is security-sensitive
- **process-argv** — Using command line arguments is security-sensitive
- **production-debug** — Delivering code in production with debug features activated is security-sensitive
- **publicly-writable-directories** — Using publicly writable directories is security-sensitive
- **session-regeneration** — A new session should be created during user authentication
- **sockets** — Using Sockets is security-sensitive
- **standard-input** — Reading the Standard Input is security-sensitive
- **strict-transport-security** — Disabling Strict-Transport-Security policy is security-sensitive
- **x-powered-by** — Disclosing fingerprints from web application technologies is security-sensitive

## Regex Internals (21)

- **anchor-precedence** — Alternatives in regular expressions should be grouped when used with anchors
- **comment-regex** — Track comments matching a regular expression
- **concise-regex** — Regular expression quantifiers and character classes should be used concisely
- **duplicates-in-character-class** — Character classes in regular expressions should not contain the same character twice
- **empty-string-repetition** — Repeated patterns in regular expressions should not match the empty string
- **existing-groups** — Replacement strings should reference existing regular expression groups
- **no-control-regex** — Regular expressions should not contain control characters
- **no-empty-after-reluctant** — Reluctant quantifiers in regular expressions should be followed by an expression that can't match the empty string
- **no-empty-alternatives** — Alternation in regular expressions should not contain empty alternatives
- **no-empty-character-class** — Empty character classes should not be used
- **no-empty-group** — Regular expressions should not contain empty groups
- **no-misleading-character-class** — Unicode Grapheme Clusters should be avoided inside regex character classes
- **no-regex-spaces** — Regular expressions should not contain multiple spaces
- **regex-complexity** — Regular expressions should not be too complicated
- **regular-expr** — Using regular expressions is security-sensitive
- **single-char-in-character-classes** — Character classes in regular expressions should not contain only one character
- **single-character-alternation** — Single-character alternations in regular expressions should be replaced with character classes
- **slow-regex** — Using slow regular expressions is security-sensitive
- **stateful-regex** — Regular expressions with the global flag should be used with caution
- **unicode-aware-regex** — Regular expressions using Unicode character classes or property escapes should enable the unicode flag
- **unused-named-groups** — Names of regular expressions named groups should be used

## Covered by TypeScript Compiler / Oxlint (28)

- **argument-type** — Arguments to built-in functions should match documented types
- **different-types-comparison** — Strict equality operators should not be used with dissimilar types
- **in-operator-type-error** — "in" should not be used with primitive types
- **new-operator-misuse** — "new" should only be used with functions and classes
- **no-delete-var** — "delete" should be used only with object properties
- **no-extra-arguments** — Function calls should not pass extra arguments
- **no-fallthrough** — Switch cases should end with an unconditional "break" statement
- **no-global-this** — The global "this" object should not be used
- **no-globals-shadowing** — Special identifiers should not be bound or assigned
- **no-implicit-dependencies** — Dependencies should be explicit
- **no-implicit-global** — Variables should be declared explicitly
- **no-invalid-regexp** — Regular expressions should be syntactically valid
- **no-literal-call** — Literals should not be used as functions
- **no-parameter-reassignment** — Initial values of parameters, caught exceptions, and loop variables should not be ignored
- **no-reference-error** — Variables should be defined before being used
- **no-require-or-define** — "import" should be used to include external code
- **no-unused-vars** — Unused local variables and functions should be removed
- **no-use-of-empty-return-value** — The return value of void functions should not be used
- **no-useless-catch** — "catch" clauses should do more than rethrow
- **no-variable-usage-before-declaration** — Variables declared with "var" should be declared before they are used
- **no-wildcard-import** — Wildcard imports should not be used
- **non-number-in-arithmetic-expression** — Arithmetic operators should only have numbers as operands
- **null-dereference** — Properties of variables with "null" or "undefined" values should not be accessed
- **unused-import** — Unnecessary imports should be removed
- **updated-const-var** — "const" variables should not be reassigned
- **updated-loop-counter** — Loop counters should not be assigned within the loop body
- **values-not-convertible-to-numbers** — Values not convertible to numbers should not be used in numeric comparisons
- **void-use** — "void" should not be used

## Style / Formatting (19)

- **arrow-function-convention** — Braces and parentheses should be used consistently with arrow functions
- **block-scoped-var** — Variables should be used in the blocks where they are declared
- **call-argument-line** — Function call arguments should not start on new lines
- **class-name** — Class names should comply with a naming convention
- **class-prototype** — Class methods should be used instead of "prototype" assignments
- **conditional-indentation** — A conditionally executed single line should be denoted by indentation
- **declarations-in-global-scope** — Variables and functions should not be declared in the global scope
- **file-header** — Track lack of copyright and license headers
- **for-in** — "for...in" loops should filter properties before acting on them
- **function-name** — Function and method names should comply with a naming convention
- **future-reserved-words** — Future reserved words should not be used as identifiers
- **label-position** — Only "while", "do", "for" and "switch" statements should be labelled
- **no-labels** — Labels should not be used
- **no-redundant-parentheses** — Redundant pairs of parentheses should be removed
- **no-same-line-conditional** — Conditionals should start on new lines
- **no-sonar-comments** — Track uses of "NOSONAR" comments
- **no-tab** — Tabulation characters should not be used
- **shorthand-property-grouping** — Shorthand object properties should be grouped at the beginning or end of an object declaration
- **variable-name** — Variable, property and parameter names should comply with a naming convention

## Framework-specific (Angular, Vue, Chai, Mocha) (6)

- **chai-determinate-assertion** — Chai assertions should have only one reason to succeed
- **disabled-timeout** — Disabling Mocha timeouts should be explicit
- **no-angular-bypass-sanitization** — Disabling Angular built-in sanitization is security-sensitive
- **no-code-after-done** — Tests should not execute any code after "done()" is called
- **no-vue-bypass-sanitization** — Disabling Vue.js built-in escaping is security-sensitive
- **stable-tests** — Tests should be stable

## HTML / Accessibility (4)

- **no-table-as-layout** — HTML "<table>" should not be used for layout purposes
- **object-alt-content** — "<object>" tags should provide an alternative content
- **table-header** — Tables should have headers
- **table-header-reference** — Table cells should reference their headers

## Crypto / TLS (10)

- **certificate-transparency** — Disabling Certificate Transparency monitoring is security-sensitive
- **encryption** — Encrypting data is security-sensitive
- **encryption-secure-mode** — Encryption algorithms should be used with secure mode and padding scheme
- **hashing** — Using weak hashing algorithms is security-sensitive
- **insecure-jwt-token** — JWT should be signed and verified with strong cipher algorithms
- **review-blockchain-mnemonic** — Wallet phrases should not be hard-coded
- **unverified-hostname** — Server hostnames should be verified during SSL/TLS connections
- **web-sql-database** — Web SQL databases should not be used
- **xml-parser-xxe** — XML parsers should not be vulnerable to XXE attacks
- **xpath** — Executing XPath expressions is security-sensitive

## Code Quality (9)

- **bitwise-operators** — Bitwise operators should not be used in boolean contexts
- **bool-param-default** — Optional boolean parameters should have default value
- **constructor-for-side-effects** — Objects should not be created to be dropped immediately without being used
- **destructuring-assignment-syntax** — Destructuring syntax should be used for assignments
- **file-name-differ-from-class** — Default export names and file names should match
- **no-empty-test-file** — Test files should contain at least one test case
- **no-selector-parameter** — Methods should not contain selector parameters
- **prefer-regexp-exec** — "RegExp.exec()" should be preferred over "String.match()"
- **too-many-break-or-continue-in-loop** — Loops should not contain more than a single "break" or "continue" statement

## Bug Detection (7)

- **deprecation** — Deprecated APIs should not be used
- **inconsistent-function-call** — Functions should be called consistently with or without "new"
- **index-of-compare-to-positive-number** — "indexOf" checks should not be for positive numbers
- **max-switch-cases** — "switch" statements should not have too many "case" clauses
- **misplaced-loop-counter** — "for" loop increment clauses should modify the loops' counters
- **no-nested-incdec** — Increment (++) and decrement (--) operators should not be used in a method call or mixed with other operators in an expression
- **no-unused-function-argument** — Unused function parameters should be removed

## Security (6)

- **confidential-information-logging** — Allowing confidential information to be logged is security-sensitive
- **dynamically-constructed-templates** — Templates should not be constructed dynamically
- **no-internal-api-use** — Users should not use internal APIs
- **no-os-command-from-path** — Searching OS commands in PATH is security-sensitive
- **post-message** — Origins should be verified during cross-origin communications
- **sql-queries** — Formatting SQL queries is security-sensitive

## Summary

| Category | Count |
|----------|-------|
| AWS / Infrastructure | 19 |
| Browser Security (runtime context needed) | 30 |
| Regex Internals | 21 |
| Covered by TypeScript Compiler / Oxlint | 28 |
| Style / Formatting | 19 |
| Framework-specific (Angular, Vue, Chai, Mocha) | 6 |
| HTML / Accessibility | 4 |
| Crypto / TLS | 10 |
| Code Quality | 9 |
| Bug Detection | 7 |
| Security | 6 |
| **Total** | **159** |
