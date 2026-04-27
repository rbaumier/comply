# New Rules — 2025-04-27 Batch

39 rules implemented from the [skills.sh ecosystem analysis](skills-raw/CANDIDATES.md).

## Svelte (9 new — 0 → 9)

| Rule | Description |
|------|-------------|
| `svelte_no_effect_for_derived` | Use `$derived` instead of `$effect` to compute values |
| `svelte_no_legacy_reactive` | Use `$state`/`$derived` instead of `$:` declarations |
| `svelte_no_on_colon_directive` | Use `onclick={}` instead of `on:click={}` (Svelte 5) |
| `svelte_no_slot_element` | Use `{#snippet}`/`{@render}` instead of `<slot>` |
| `svelte_no_svelte_component` | Use `<Component>` instead of `<svelte:component this={}>` |
| `svelte_no_store_import` | Use `$state` classes instead of `svelte/store` imports |
| `svelte_no_index_as_each_key` | Don't use index as key in `{#each}` blocks |
| `svelte_prefer_create_context` | Use `createContext` instead of `setContext`/`getContext` |
| `svelte_no_class_directive` | Use clsx-style in `class` attr instead of `class:` directive |

## GraphQL (7 new — 1 → 8)

| Rule | Description |
|------|-------------|
| `graphql_require_operation_name` | All operations must be named (no anonymous) |
| `graphql_no_inline_arguments` | Use variables, not hardcoded values in operations |
| `graphql_require_id_field` | Types implementing Node must include `id: ID!` |
| `graphql_prefer_input_type` | Mutations should use input types, not many args |
| `graphql_list_non_null_items` | Lists should use `[Type!]!` (non-null items) |
| `graphql_require_description` | Types and fields must have descriptions |
| `graphql_no_float_for_money` | Don't use `Float` for monetary values |

## Hono (5 new — 8 → 13)

| Rule | Description |
|------|-------------|
| `hono_no_unvalidated_body` | `c.req.json()` without validator middleware |
| `hono_error_leaks_stack` | Error handler exposing `err.stack`/`err.message` |
| `hono_no_hardcoded_cors_origin` | CORS origin should use env var, not string literal |
| `hono_jwt_secret_hardcoded` | JWT secret hardcoded in code |
| `hono_no_get_with_body` | GET/HEAD routes shouldn't parse body |

## Prisma (3 new — 7 → 10)

| Rule | Description |
|------|-------------|
| `prisma_no_findmany_without_take` | `findMany()` without `take`/`first` returns unbounded results |
| `prisma_no_nested_include_depth` | Deeply nested `include` (>3 levels) |
| `prisma_require_transaction_for_multi_write` | Multiple write ops without `$transaction` |

## Angular (3 new — 10 → 13)

| Rule | Description |
|------|-------------|
| `angular_prefer_signals` | Use `signal()` instead of `BehaviorSubject` for component state |
| `angular_no_topromise` | Use `firstValueFrom` instead of deprecated `.toPromise()` |
| `angular_require_onpush` | Components should use `OnPush` change detection |

## NestJS (3 new — 8 → 11)

| Rule | Description |
|------|-------------|
| `nestjs_controller_return_type` | Controllers must have explicit return types |
| `nestjs_no_entity_in_controller` | Controllers should not import ORM entities |
| `nestjs_no_forwardref_abuse` | Avoid circular deps with `forwardRef` — restructure instead |

## Next.js (1 new — 15 → 16)

| Rule | Description |
|------|-------------|
| `next_no_redirect_in_try_catch` | `redirect()` should not be wrapped in try/catch |

## Node.js (2 new — 14 → 16)

| Rule | Description |
|------|-------------|
| `node_no_unhandled_rejection` | `process.on('unhandledRejection')` without exit |
| `node_prefer_stream_pipeline` | Use `stream.pipeline()` instead of `.pipe()` chaining |

## Security (3 new — 9 → 12)

| Rule | Description |
|------|-------------|
| `security_no_cors_reflect_origin` | CORS reflecting request origin without allowlist |
| `security_no_password_in_log` | Logging variables named password/secret/token |
| `security_cookie_no_samesite_none` | Cookie with `SameSite=None` without `Secure` |

## Rust (3 new — 58 → 61)

| Rule | Description |
|------|-------------|
| `rust_no_todo_macro` | `todo!()` in non-test code |
| `rust_prefer_tracing_over_log` | Use `tracing` instead of `log` crate |
| `rust_no_sleep_in_test` | `thread::sleep`/`tokio::time::sleep` in test code |

## Docker Compose (2 new — 7 → 9)

| Rule | Description |
|------|-------------|
| `compose_no_network_host` | `network_mode: host` bypasses network isolation |
| `compose_healthcheck_required` | Services should define healthcheck |
