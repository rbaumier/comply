# Candidate Rules from Skills Analysis

## Methodology
- 38 skill files downloaded from skills.sh ecosystem
- Cross-referenced against 1580 existing comply rules
- Focus on gaps: Svelte (0), GraphQL (1), + additional rules for Angular, NestJS, Hono, Prisma, Node, Security

## Legend
- ✅ = Selected for implementation
- ❌ = Rejected (duplicate or low value)
- ⚠️ = Borderline (implement if time allows)

---

## SVELTE (0 existing → 10 new)
Source: sveltejs/ai-tools svelte-core-bestpractices, ejirocodes/agent-skills svelte5-best-practices

| # | Rule | Description | Status |
|---|------|-------------|--------|
| 1 | svelte_no_effect_for_derived | Use $derived instead of $effect to compute values | ✅ |
| 2 | svelte_no_legacy_reactive | Use $state/$derived instead of $: declarations | ✅ |
| 3 | svelte_no_on_colon_directive | Use onclick={} instead of on:click={} (Svelte 5) | ✅ |
| 4 | svelte_no_slot_element | Use {#snippet}/{@render} instead of <slot> | ✅ |
| 5 | svelte_no_svelte_component | Use <Component> instead of <svelte:component this={}> | ✅ |
| 6 | svelte_no_store_import | Use $state classes instead of svelte/store imports | ✅ |
| 7 | svelte_prefer_state_raw | Use $state.raw for objects only reassigned (not mutated) | ⚠️ hard to detect |
| 8 | svelte_no_index_as_each_key | Don't use index as key in {#each} blocks | ✅ |
| 9 | svelte_prefer_create_context | Use createContext instead of setContext/getContext | ✅ |
| 10 | svelte_no_class_directive | Use clsx-style in class attr instead of class: directive | ✅ |

## GRAPHQL (1 existing → 8 new)
Source: apollographql/skills graphql-schema + graphql-operations

| # | Rule | Description | Status |
|---|------|-------------|--------|
| 1 | graphql_require_operation_name | All operations must be named (no anonymous) | ✅ |
| 2 | graphql_no_inline_arguments | Use variables, not hardcoded values in operations | ✅ |
| 3 | graphql_require_id_field | Types implementing Node must include id: ID! | ✅ |
| 4 | graphql_prefer_input_type | Mutations should use input types, not many args | ✅ |
| 5 | graphql_list_non_null_items | Lists should use [Type!]! (non-null items) | ✅ |
| 6 | graphql_require_description | Types and fields must have descriptions | ✅ |
| 7 | graphql_no_float_for_money | Don't use Float for monetary values | ✅ |
| 8 | graphql_prefer_enum_over_string | Use enums for fixed value sets | ⚠️ |

## ADDITIONAL HONO (8 existing → 6 new)
Source: yusukebe/hono-skill

| # | Rule | Description | Status |
|---|------|-------------|--------|
| 1 | hono_no_unvalidated_body | c.req.json() without validator middleware | ✅ |
| 2 | hono_error_leaks_stack | Error handler exposing err.stack/err.message | ✅ |
| 3 | hono_no_hardcoded_cors_origin | CORS origin should use env var, not string literal | ✅ |
| 4 | hono_jwt_secret_hardcoded | JWT secret hardcoded in code | ✅ |
| 5 | hono_no_get_with_body | GET/HEAD routes shouldn't parse body | ✅ |
| 6 | hono_prefer_factory_handlers | Use createHandlers() for type-safe middleware | ⚠️ |

## ADDITIONAL PRISMA (7 existing → 4 new)
Source: prisma/skills prisma-client-api

| # | Rule | Description | Status |
|---|------|-------------|--------|
| 1 | prisma_no_findmany_without_take | findMany() without take/first returns unbounded results | ✅ |
| 2 | prisma_no_queryraw_string_concat | $queryRaw with string concat (use Prisma.sql tagged template) | ❌ dup of prisma_no_raw_query_interpolation |
| 3 | prisma_no_nested_include_depth | Deeply nested include (>3 levels) | ✅ |
| 4 | prisma_require_transaction_for_multi_write | Multiple write ops without $transaction | ✅ |

## ADDITIONAL ANGULAR (10 existing → 5 new)
Source: angular/skills, analogjs/angular-skills

| # | Rule | Description | Status |
|---|------|-------------|--------|
| 1 | angular_prefer_signals | Use signal() instead of BehaviorSubject for component state | ✅ |
| 2 | angular_no_topromise | Use firstValueFrom instead of deprecated .toPromise() | ✅ |
| 3 | angular_require_onpush | Components should use OnPush change detection | ✅ |
| 4 | angular_no_any_in_service | Service methods should not use any type | ❌ dup concept of angular_no_any_in_template |
| 5 | angular_no_manual_subscription_in_template | Use async pipe instead of manual subscribe in component | ❌ dup of angular_no_subscribe_in_template |

## ADDITIONAL NESTJS (8 existing → 4 new)
Source: kadajett/agent-nestjs-skills

| # | Rule | Description | Status |
|---|------|-------------|--------|
| 1 | nestjs_controller_return_type | Controllers must have explicit return types | ✅ |
| 2 | nestjs_no_entity_in_controller | Controllers should not import ORM entities | ✅ |
| 3 | nestjs_no_console_in_production | Use Logger service instead of console.log | ⚠️ generic |
| 4 | nestjs_no_forwardref_abuse | Avoid circular deps with forwardRef — restructure instead | ✅ |

## ADDITIONAL NEXT.JS (15 existing → 4 new)
Source: wshobson/agents nextjs-app-router-patterns

| # | Rule | Description | Status |
|---|------|-------------|--------|
| 1 | next_no_use_client_with_metadata | "use client" files should not export metadata/generateMetadata | ❌ dup of react_no_metadata_export_in_client |
| 2 | next_require_loading_ui | Route segments with async should have loading.tsx | ⚠️ hard to detect |
| 3 | next_no_redirect_in_try_catch | next/navigation redirect() should not be wrapped in try/catch | ✅ |
| 4 | next_prefer_server_action_over_api | Prefer server actions over API routes for mutations | ⚠️ too opinionated |

## ADDITIONAL NODE (14 existing → 4 new)
Source: sickn33/antigravity-awesome-skills nodejs-best-practices

| # | Rule | Description | Status |
|---|------|-------------|--------|
| 1 | node_no_unhandled_rejection | process.on('unhandledRejection') without exit | ✅ |
| 2 | node_prefer_stream_pipeline | Use stream.pipeline() instead of pipe() chaining | ✅ |
| 3 | node_no_event_emitter_leak | EventEmitter with > 10 listeners without setMaxListeners | ⚠️ |
| 4 | node_no_blocking_main_thread | Sync fs/crypto calls in HTTP handler context | ❌ dup of node_no_sync |

## ADDITIONAL SECURITY (9 existing → 4 new)
Source: supercent-io/skills-template security-best-practices

| # | Rule | Description | Status |
|---|------|-------------|--------|
| 1 | security_no_cors_reflect_origin | CORS reflecting request origin without allowlist | ✅ |
| 2 | security_no_jwt_none_algorithm | JWT with algorithm: 'none' | ❌ dup of no_insecure_jwt |
| 3 | security_no_password_in_log | Logging variables named password/secret/token | ✅ |
| 4 | security_cookie_no_samesite_none | Cookie with SameSite=None without Secure | ✅ |

## ADDITIONAL RUST (58 existing → 4 new)
Source: apollographql/skills rust-best-practices, zhanghandong/rust-skills m15-anti-pattern

| # | Rule | Description | Status |
|---|------|-------------|--------|
| 1 | rust_no_todo_macro | todo!() in non-test code | ✅ |
| 2 | rust_prefer_tracing_over_log | Use tracing instead of log crate | ✅ |
| 3 | rust_no_manual_impl_display | Use thiserror/derive_more instead of manual Display impl for errors | ⚠️ |
| 4 | rust_no_sleep_in_test | thread::sleep/tokio::time::sleep in test code | ✅ |

## ADDITIONAL DOCKER/COMPOSE (61 existing → 3 new)
Source: sickn33/antigravity-awesome-skills docker-expert

| # | Rule | Description | Status |
|---|------|-------------|--------|
| 1 | dockerfile_no_env_secrets | ENV with password/secret/key variable names | ❌ dup of dockerfile_no_secrets_in_env |
| 2 | compose_no_network_host | network_mode: host bypasses network isolation | ✅ |
| 3 | compose_healthcheck_required | Services should define healthcheck | ✅ |

---

## SUMMARY

| Category | Existing | New (selected ✅) | Total |
|----------|----------|-------------------|-------|
| Svelte | 0 | 9 | 9 |
| GraphQL | 1 | 7 | 8 |
| Hono | 8 | 5 | 13 |
| Prisma | 7 | 3 | 10 |
| Angular | 10 | 3 | 13 |
| NestJS | 8 | 3 | 11 |
| Next.js | 15 | 1 | 16 |
| Node | 14 | 2 | 16 |
| Security | 9 | 3 | 12 |
| Rust | 58 | 3 | 61 |
| Docker/Compose | 61 | 2 | 63 |
| **TOTAL** | **191** | **41** | **232** |
