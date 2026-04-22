# Règles comply — État au 2026-04-22

Toutes les règles sont implémentées.

---

## Zod (11 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `zod-prefer-safe-parse` | Facile | ✅ |
| `zod-string-min-1-required` | Moyen | ✅ |
| `zod-trim-before-min` | Facile | ✅ |
| `zod-prefer-discriminated-union` | Moyen | ✅ |
| `zod-refine-requires-path` | Moyen | ✅ |
| `zod-brand-ids` | Difficile | ✅ |
| `zod-require-error-messages` | Facile | ✅ |
| `zod-no-optional-nullable-chain` | Facile | ✅ |
| `zod-validate-env-at-startup` | Difficile | ✅ |
| `zod-transform-requires-pipe` | Moyen | ✅ |
| `drizzle-zod-prefer-generated-schema` | Difficile | ✅ |

---

## TanStack Query (12 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `tanstack-query-no-is-loading` | Facile | ✅ |
| `tanstack-query-no-cache-time` | Facile | ✅ |
| `tanstack-query-no-use-error-boundary` | Facile | ✅ |
| `tanstack-query-no-keep-previous-data-prop` | Facile | ✅ |
| `tanstack-query-no-query-callbacks` | Facile | ✅ |
| `tanstack-query-require-stale-time` | Moyen | ✅ |
| `tanstack-query-fn-must-throw-on-error` | Moyen | ✅ |
| `tanstack-query-key-includes-params` | Difficile | ✅ |
| `tanstack-query-prefer-query-options` | Difficile | ✅ |
| `tanstack-query-no-enabled-true` | Facile | ✅ |
| `tanstack-query-prefer-suspense-query` | Difficile | ✅ |
| `tanstack-query-prefer-key-factory` | Difficile | ✅ |

---

## TanStack Start (6 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `tanstack-start-server-fn-requires-validation` | Moyen | ✅ |
| `tanstack-start-server-fn-requires-auth` | Moyen | ✅ |
| `tanstack-start-server-fn-file-convention` | Facile | ✅ |
| `tanstack-start-require-validate-search` | Moyen | ✅ |
| `tanstack-start-loader-stale-time` | Moyen | ✅ |
| `tanstack-start-no-client-import-in-server-fn` | Facile | ✅ |

---

## Vue (11 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `vue-script-setup-required` | Facile | ✅ |
| `vue-sfc-section-order` | Facile | ✅ |
| `vue-no-v-html-unsafe` | Moyen | ✅ |
| `vue-prefer-v-else` | Moyen | ✅ |
| `vue-require-lifecycle-cleanup` | Moyen | ✅ |
| `vue-pinia-store-to-refs` | Moyen | ✅ |
| `vue-define-emits-typed` | Facile | ✅ |
| `vue-prefer-computed` | Difficile | ✅ |
| `vue-markraw-for-third-party` | Difficile | ✅ |
| `vue-no-mutate-prop` | Facile | ✅ |
| `vue-url-state-for-filters` | Difficile | ✅ |

---

## Tailwind (7 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `tailwind-prefer-cn-utility` | Moyen | ✅ |
| `tailwind-no-apply-for-variants` | Moyen | ✅ |
| `tailwind-no-important-modifier` | Facile | ✅ |
| `tailwind-no-arbitrary-z-index` | Facile | ✅ |
| `tailwind-prefer-size-shorthand` | Moyen | ✅ |
| `tailwind-no-magic-spacing` | Moyen | ✅ |
| `tailwind-read-theme-before-classes` | Difficile | ✅ |

---

## React (10 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `react-server-action-requires-validation` | Moyen | ✅ |
| `react-server-action-requires-auth` | Moyen | ✅ |
| `react-prefer-use-transition` | Moyen | ✅ |
| `react-no-inline-default-prop` | Moyen | ✅ |
| `react-prefer-react-cache` | Difficile | ✅ |
| `react-no-derived-state-in-effect` | Moyen | ✅ |
| `react-passive-event-listeners` | Facile | ✅ |
| `react-no-sequential-await-in-component` | Difficile | ✅ |
| `react-use-state-initializer-function` | Moyen | ✅ |
| `react-hoist-static-jsx` | Difficile | ✅ |

---

## i18n (7 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `i18n-no-hardcoded-string-in-jsx` | Moyen | ✅ |
| `i18n-no-concat-translation-key` | Facile | ✅ |
| `i18n-no-string-concat-with-translation` | Facile | ✅ |
| `i18n-prefer-intl-api` | Facile | ✅ |
| `i18n-no-manual-pluralization` | Moyen | ✅ |
| `i18n-no-unnecessary-trans-component` | Facile | ✅ |
| `i18n-prefer-logical-css-properties` | Facile | ✅ |

---

## Sécurité (10 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `no-prototype-pollution` | Moyen | ✅ |
| `no-mass-assignment` | Moyen | ✅ |
| `no-open-redirect` | Moyen | ✅ |
| `no-error-details-in-response` | Moyen | ✅ |
| `no-shell-exec` | Moyen | ✅ |
| `no-path-traversal` | Moyen | ✅ |
| `no-unvalidated-url-redirect` | Moyen | ✅ |
| `no-ssrf-fetch` | Difficile | ✅ |
| `no-regex-user-input` | Facile | ✅ (= `no-new-regex-with-variable`) |
| `audit-log-required-fields` | Difficile | ✅ |

---

## TypeScript / Architecture (7 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `no-default-export` | Facile | ✅ |
| `no-unchecked-json-parse` | Moyen | ✅ |
| `ts-prefer-satisfies` | Moyen | ✅ |
| `no-conditional-async-return` | Moyen | ✅ |
| `prefer-promise-all` | Moyen | ✅ |
| `ts-prefer-using-declaration` | Moyen | ✅ |
| `no-double-cast` | Facile | ✅ |

---

## Drizzle ORM (5 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `drizzle-returning-on-insert-update` | Moyen | ✅ |
| `drizzle-no-sql-raw-with-variable` | Facile | ✅ |
| `drizzle-no-select-without-limit` | Difficile | ✅ |
| `drizzle-chunk-large-batch-insert` | Difficile | ✅ |
| `drizzle-no-push-in-production` | Facile | ✅ |

---

## Database SQL (5 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `sql-create-index-concurrently` | Facile | ✅ |
| `sql-require-transaction-timeout` | Difficile | ✅ |
| `sql-nullable-requires-comment` | Moyen | ✅ |
| `sql-no-between-timestamp` | Facile | ✅ |
| `sql-advisory-lock-prefer-xact` | Facile | ✅ |

---

## API Design (5 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `api-no-array-root-response` | Facile | ✅ |
| `api-list-requires-pagination` | Moyen | ✅ |
| `api-no-boolean-field-in-response` | Difficile | ✅ |
| `api-deprecation-headers` | Difficile | ✅ |
| `api-import-from-public-index` | Moyen | ✅ (= `layer-import-boundary`) |

---

## Rust (10 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `rust-prefer-cow` | Difficile | ✅ |
| `rust-no-mutex-in-single-threaded` | Difficile | ✅ |
| `rust-vec-with-capacity` | Moyen | ✅ |
| `rust-prefer-channel-over-arc-mutex-vec` | Moyen | ✅ |
| `rust-anyhow-context-on-question-mark` | Moyen | ✅ |
| `rust-prefer-once-lock` | Facile | ✅ |
| `rust-must-use-on-result-fn` | Moyen | ✅ |
| `rust-unsafe-ffi-isolation` | Moyen | ✅ |
| `rust-thiserror-for-lib` | Moyen | ✅ |
| `rust-prefer-fast-hasher` | Moyen | ✅ |

---

## Testing (5 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `testing-prefer-msw` | Facile | ✅ |
| `testing-prefer-test-each` | Difficile | ✅ |
| `testing-no-and-in-test-name` | Facile | ✅ |
| `testing-no-undefined-mock-var` | Moyen | ✅ |
| `testing-no-real-external-service` | Moyen | ✅ |

---

## Better Auth (7 règles)

| ID | Faisabilité | État |
|----|-------------|------|
| `better-auth-no-disable-csrf` | Facile | ✅ |
| `better-auth-no-disable-origin-check` | Facile | ✅ |
| `better-auth-require-secure-cookies` | Moyen | ✅ |
| `better-auth-require-rate-limit` | Moyen | ✅ |
| `better-auth-plugin-import-path` | Facile | ✅ |
| `better-auth-trusted-providers` | Moyen | ✅ |
| `better-auth-middleware-requires-headers` | Moyen | ✅ |

---

## Récapitulatif

| Domaine | Total | ✅ Done |
|---------|-------|---------|
| Zod | 11 | 11 |
| TanStack Query | 12 | 12 |
| TanStack Start | 6 | 6 |
| Vue | 11 | 11 |
| Tailwind | 7 | 7 |
| React | 10 | 10 |
| i18n | 7 | 7 |
| Sécurité | 10 | 10 |
| TypeScript/Arch | 7 | 7 |
| Drizzle ORM | 5 | 5 |
| Database SQL | 5 | 5 |
| API Design | 5 | 5 |
| Rust | 10 | 10 |
| Testing | 5 | 5 |
| Better Auth | 7 | 7 |
| **Total** | **118** | **118** |
