# Decision log — `comply review` (LLM review pipeline)

Date: 2026-04-21
Status: grilling complete, ready to plan
Scope: design decisions for the new `comply review` subcommand — LLM-driven PR/diff review, local-first, pre-push manual invocation.

## Summary

`comply review` is a **new subcommand**, separate from the existing `comply --with-llm` linter. It reviews a diff (reusing comply's existing diff-selection flags), runs a multi-stage LLM pipeline (Haiku triage + skill selection → Sonnet findings + summary), injects relevant user skills from `~/.claude/skills`, scores each finding by severity (boosted by blast radius from a partial 1-hop dep graph), and outputs a markdown report.

Runs locally via the `ai-sdk-provider-claude-code` provider (Max subscription, no API key).

## Decisions

| # | Decision | Choice | Rationale |
|---|---|---|---|
| **Purpose & scope** | | | |
| 1.1 | Product shape | New subcommand `comply review` | Invariants & cache incompatible with `--with-llm` (stateless per-file linter vs stateful per-PR reviewer) |
| 1.2 | Diff source | Reuse existing flags (`--working-tree`, `--staged`, `--last-commit`, `--commit`, `--range`) | Zero reinvention, UX consistency |
| 1.3a | Usage mode | Manual pre-push (no pre-commit hook) | LLM latency + FP rate incompatible with blocking hook |
| 1.3b | Models | Haiku (triage) + Sonnet (findings+summary); `--deep` adds Opus | Opus overkill for local, Sonnet 4.6 sufficient for review quality |
| 1.3c | Provider | `ai-sdk-provider-claude-code` (Max sub) | Zero API key, minimal friction |
| **Architecture** | | | |
| 2.1a | Languages v1 | TS/TSX/JS + Rust | 80% of volume, mature resolvers; Vue is v2 |
| 2.1b | Dep graph scope | Partial 1-hop, on-demand | Full repo crawl = 10-30s; 1-hop = <2s and already strong signal |
| 2.1c | Graph cache | None at v1 | YAGNI, rebuild is cheap |
| 2.2 | ast-grep | **Rejected** | Wrong tool for link resolution; ~500 tree-sitter rules already cover pattern matching |
| 2.3 | Separate regex rules layer | **Rejected** | TextCheck + AstCheck already cover this |
| 2.4 | Cross-file symbol index | **Rejected** v1 | 1-hop dep graph sufficient; LSP-grade = v2 if concrete need |
| **Chunking** | | | |
| 3.1 | Chunk content | Hunks widened via tree-sitter to enclosing function/class | Zero semantic context loss, 3-5× fewer tokens than full file |
| 3.2 | Cross-file context | 1 target file + exported signatures + caller call-sites (≤ 2k tokens) | Catches cross-file bugs without multiplying tokens or breaking parallelism |
| 3.3 | Max chunk size | ~8k tokens input, split by top-level items if exceeded | Predictable latency, clean cache keys |
| 3.4 | Parallelism | 5 concurrent Sonnet calls | Rate-limit friendly, aligned with existing worker |
| **Triage (Haiku)** | | | |
| 4.1 | Triage input | Diff + metadata (path, size, % changed) | Ultra cheap, no need for full file |
| 4.2 | Deterministic pre-filter | Active (binary ext, lockfiles, `.min.js`, `@generated`, pure renames, whitespace-only) | ~50% of files filtered without LLM |
| 4.3 | Re-review of APPROVED | Never | Otherwise triage is pointless; log APPROVED for audit |
| **Scoring** | | | |
| 5.1a | Scoring unit | **Both**: categorical severity per finding + quality_score 1-10 per file | LLMs poor at fine ordinals per finding; file-level quality useful for summary |
| 5.1b | Threshold filter | On severity, not quality_score | Quality score is displayed info, not a filter |
| 5.1c | Blast radius target | Boost finding **severity**, not file quality_score | A file's intrinsic quality doesn't change with its connection count |
| 5.2 | Blast radius formula | `severity_boost = min(1, log2(importers+1)/3)` | Additive, bounded, log saturation avoids over-weighting utils.ts |
| 5.3 | Default threshold & max findings | severity ≥ medium; max 15 shown, sorted by score desc | Configurable via `comply.toml` `[review]` |
| 5.4 | Dedup | Exact on `(path, line, rule_id or hash(message))` | Semantic dedup hides real problems |
| **Skill injection** | | | |
| 6.1 | Skill selection | **LLM matching** by Haiku, merged with triage verdict | Static pattern matching too fragile to scope correctly |
| 6.2 | Injected content | Full `SKILL.md` body | No curation, simple |
| 6.3 | Token budget for skills | **No cap** | Sonnet 200k context is large; accept the volume |
| 6.4 | Skills in triage | No | Triage is verdict-only, no need for skills |
| 6.5 | Injection format | XML-ish `<review_guidelines><guideline source="..."/></review_guidelines>` + `<file_under_review>` | Clear structure, clean separation |
| 6.6 | Skill eligibility | Explicit **whitelist** declared in comply | Avoids injecting non-review-aware skills (e.g. `grill-me`) |
| 6.7/6.8 | Prompt caching | **None** inter-chunks via `ai-sdk-provider-claude-code` (verified) | Provider doesn't expose `cache_control`; subprocess CLIs are isolated |
| Q_CACHE | Caching strategy | **α — status quo**, accept token volume cost | Keep zero-API-key path; β (Anthropic SDK direct) or γ (extraction) as fallback if rate limits become an issue |
| **Failure modes** | | | |
| 7.1 | Invalid Sonnet JSON | Skip + log, no retry | Double cost for low gain |
| 7.2 | Rate-limit 429 | Exponential retry 1s/2s/4s × 3 then skip | Aligned with existing worker |
| 7.3 | Ctrl-C | Show partial results + banner; no cache write for incomplete chunks | Avoid garbage in DB |
| 7.4 | Diff too large | Hard cap 50 NEEDS_REVIEW files post-triage; refuse above with message | No sloppy reviews |
| **Cache (SQLite)** | | | |
| 8.1 | Cache key | `sha256(file_content + skills_used + model + prompt_version)`; invalidates when skill body changes | Skill tweak should invalidate legitimately |
| 8.2 | TTL | None; `--no-cache` for forced re-run | YAGNI |
| **Output** | | | |
| 9.1 | Default format | Markdown on stdout; `--format json` for NDJSON | Review is human-read; miette format inappropriate |
| 9.2 | Summary placement | At the top (TL;DR) | User scans then descends |
| 9.3 | Exit codes | 0 = clean; 1 = findings ≥ threshold; 2 = system error | Aligned with classic comply |

## Residual risks (not blockers — track during implementation)

| Risk | Impact | Mitigation |
|---|---|---|
| Rate-limit on Max sub for large diffs | High (blocks review) | 50-file cap post-triage mitigates; confirm empirically; fallback = β (Anthropic SDK direct) |
| Haiku skill-matching quality | Medium (noise in Sonnet prompt) | Whitelist bounds the set; fallback = static pattern matching in v2 |
| Per-file quality_score drift between runs | Low (display-only, not filter) | Document as advisory; not used in decision logic |
| Tree-sitter hunk widening edge cases (macros, JSX fragments, Rust macro bodies) | Low (few chunks affected) | Fallback to full-file chunk when widening fails |

## Out of scope (v2+)

- Vue and other languages beyond TS/TSX/JS/Rust
- Cross-file symbol index (LSP-grade)
- Dep graph caching (incremental build, mtime-keyed)
- CI integration with API-key backend (β option)
- Prompt caching via Anthropic SDK direct
- Pre-commit hook integration
