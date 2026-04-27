---
issue: ISS-001
agent: opus-reviewer
status: changes-requested
date: 2026-04-27
---

## Summary

The plan addresses all 4 sub-problems from the issue (TanStack Router, shadcn/ui, root config files, email templates) and the source-file claims about line numbers and signatures are accurate. However, the plan ships two undecided alternatives for email templates (Step 6, "Option A" and "Option B") without picking one, scope-creeps a TanStack `root_files` field that the issue did not ask for, and underestimates the dead-export coverage gap — fixing `vite.config.ts` alone won't drain the 120 dead-export FPs because TanStack route files (which the plan does NOT exempt in `dead-export`) are part of that count.

## Findings

### critical — Step 6 ships two competing solutions instead of one

- **File**: `/Users/rbaumier/www/comply/docs/plans/2026-04-27-ISS-001.md` (Step 6, lines 136-162)
- **Description**: Step 6 lists "Option B (ship first)" and "Option A (escape hatch)" as if both are part of the same step, but they are different mechanisms (framework TOML vs. user `comply.toml` config schema). The sequencing table only allots "~20 FP" / "Option B first" — yet Step 7 asks for `comply_toml_extra_entry_dir()` test which is Option A. Without a decision the implementer can't know what to build, and the L estimate covers only one path.
- **Recommendation**: Pick one. Issue §4 lists react-email/templates as "Fix possible" with both options — recommend going with Option B (`react-email.toml`) only and dropping Option A from this plan; if a project-level escape hatch is wanted, file it as a follow-up issue.
- **Status**: open

### critical — `dead-export` will still flag TanStack route exports unless magic_exports is wired through

- **File**: `/Users/rbaumier/www/comply/src/rules/dead_export/text.rs:56-66`
- **Description**: `dead-export` already consults `framework_magic_exports()`, so the plan's TanStack `magic_exports.names` (Step 2) automatically helps. But many TanStack route files use `export const Route = createFileRoute(...)` AND auxiliary exports like `loader`, `component` — which only get exempted via the magic-exports list. The plan correctly lists these names, BUT `dead-export` only runs after `is_entry_point` returns false (line 35). Since `is_entry_point` in `dead-export` is hard-coded to `main`/`index` at the root and the plan does NOT propose to broaden it, route files in `/routes/` are processed by `dead-export` and rely entirely on the magic-export list. That is fine for `Route`, `loader`, `component`, but a route file that exports a custom helper (`export const myHelper = ...`) will still be flagged. The plan should either (a) make `dead-export` skip files inside `framework_entry_dirs()`, or (b) explicitly accept that custom exports in route files remain flagged. The current plan is silent on this.
- **Recommendation**: Add a sub-step: in `dead_export/text.rs`, after the `is_entry_point` check, also skip files whose path matches any `project.framework_entry_dirs()` fragment. Document in the plan that this is the symmetric counterpart to Step 5.
- **Status**: open

### major — Step 4 description doesn't match the actual `dead-export` skip pattern

- **File**: `/Users/rbaumier/www/comply/docs/plans/2026-04-27-ISS-001.md` (Step 4, lines 102-114)
- **Description**: The plan says "after the test-file skip (line 32-34), add: `if ctx.file.is_config_file()`". But the test-file skip in `dead_export/text.rs:32` is `if ctx.file.path_segments.in_test_dir` — which is a `FileCtx` field, not a free function. The plan also proposes "Move `is_config_file()` … into `FileCtx`". That refactor is fine but the plan understates it: `is_config_file` currently lives as a private free fn in `unused_file/text.rs`, and there are TWO existing call sites in that file (lines 47 and 78). Moving it to `FileCtx` requires updating both call sites in `unused_file/text.rs` plus adding the new call site in `dead_export/text.rs`. The plan doesn't list those call-site updates. Also, the `unused_file` test `flags_unreachable_file` and friends use `is_config_file(path)` indirectly — they'll still work, but the plan should flag the refactor surface explicitly.
- **Recommendation**: Update Step 4 to enumerate: (1) add `is_config_file` method to `FileCtx`, (2) replace the two call sites in `unused_file/text.rs` (lines 47, 78), (3) add the new call site in `dead_export/text.rs` after the `path_segments.in_test_dir` check. Alternative: keep it as a free fn in a shared module — simpler if `FileCtx` doesn't already carry the path metadata needed.
- **Status**: open

### major — `root_files = ["app.config", "vite.config"]` in tanstack-router.toml is scope creep

- **File**: `/Users/rbaumier/www/comply/docs/plans/2026-04-27-ISS-001.md` (Step 2, line 51)
- **Description**: The TanStack TOML proposes `root_files = ["app.config", "vite.config"]`. But `vite.config` is already covered by the existing `vite.toml` (verify by reading it), and the planned `is_config_file()` extension to `dead-export` (Step 4) already handles all `*.config.*` files generically. Listing them per-framework duplicates coverage and creates ordering dependencies on which framework matches.
- **Recommendation**: Drop the `root_files` array from `tanstack-router.toml`. It's redundant with `is_config_file()` after Step 4 lands.
- **Status**: open

### major — `entry_points.files` already exists; Step 1 misstates the schema change

- **File**: `/Users/rbaumier/www/comply/src/frameworks/mod.rs:32-37`
- **Description**: Step 1 says "Add … `files: Vec<String>` to `Detection` struct" — that's correct, `Detection` only has `dependencies` today. But Step 1 also implies `entry_points.files` is being added; `EntryPoints` already has `files: Vec<String>` (mod.rs:34). The plan's diff at lines 17-21 mixes new and existing fields without flagging which is which. The only actual schema additions are: `Detection.files` and `EntryPoints.file_suffixes`.
- **Recommendation**: Restate Step 1 as exactly two additive fields: `Detection.files` (for `components.json` style presence detection) and `EntryPoints.file_suffixes` (for `*.lazy.tsx`). Everything else is already in place.
- **Status**: open

### major — `detect_frameworks` signature change has wider blast radius than claimed

- **File**: `/Users/rbaumier/www/comply/src/frameworks/mod.rs:80` and tests at 110-136
- **Description**: Plan claims "single call site at `project/mod.rs:384`". Verified: that's the only production call site. BUT four existing tests in `frameworks/mod.rs` (`detects_nextjs`, `detects_jest_in_dev_deps`, `no_match_with_empty_pkg`, `multiple_frameworks_match`) all call `detect_frameworks(&pkg)` with one argument. Changing the signature to `(pkg, project_root)` breaks all four. The plan must either (a) add a default `None` via an Option or (b) update all four tests. Acceptable either way, but the plan should call it out.
- **Recommendation**: Add an explicit note in Step 1 that the four existing tests in `frameworks/mod.rs` need their call sites updated, OR keep `detect_frameworks(pkg)` as the public surface and add `detect_frameworks_with_root(pkg, root)` as a new function — pick one and document.
- **Status**: open

### minor — `is_entry_point` in dead-export takes `Option<&Path>`, not `&ProjectCtx`

- **File**: `/Users/rbaumier/www/comply/src/rules/dead_export/text.rs:91`
- **Description**: The plan's Step 4 snippet uses `ctx.file.is_config_file()` which is fine, but be aware `dead_export::is_entry_point` has a different signature than `unused_file::is_entry_point` (the former takes `Option<&Path>`, the latter takes `&ProjectCtx`). If the implementer tries to unify them or extend `dead-export`'s entry-point check (per the critical finding above), they need access to `&ProjectCtx`. The current `Check::check` call site (line 35) already has `ctx.project`, so this is solvable, but worth noting.
- **Recommendation**: When extending `dead-export` to skip framework entry dirs (per critical finding), pass `&ProjectCtx` into `is_entry_point` rather than threading another arg.
- **Status**: open

### minor — `routeTree.gen.ts` exemption likely needs path-suffix matching, not exact filename

- **File**: `/Users/rbaumier/www/comply/docs/plans/2026-04-27-ISS-001.md` (Step 2, line 49)
- **Description**: `entry_points.files` is matched against `path.file_name()` (verified in `unused_file/text.rs:83-91`). `routeTree.gen.ts` as a bare filename will match. But TanStack also generates files at user-configurable paths (some configs put it in `src/routeTree.gen.ts`, others elsewhere). Since match is on `file_name()` only, location is irrelevant — fine for this case. No change needed but the plan should not promise to handle all TanStack codegen output, only `routeTree.gen.{ts,tsx}`.
- **Recommendation**: No code change. Tighten plan wording to "`routeTree.gen.{ts,tsx}` regardless of directory".
- **Status**: open

### minor — Step 7 integration test asserts "zero `dead-export`" but TanStack routes export `Route`

- **File**: `/Users/rbaumier/www/comply/docs/plans/2026-04-27-ISS-001.md` (Step 7, lines 188-202)
- **Description**: The fixture's `dashboard.lazy.tsx` and `__root.tsx` will export `Route`, `component`, etc. `dead-export` will check them unless they're skipped via `is_entry_point` OR every export name is in the magic-exports list. The plan's TanStack TOML covers all the standard names, so this should pass — but only if Step 5's suffix-based `is_entry_point` extension is also applied to `dead-export` (it isn't, currently). The test as written may pass for `dashboard.lazy.tsx` (because all exports are in magic_exports) but fail if the fixture adds any custom export.
- **Recommendation**: Either keep fixture exports strictly within the magic_exports list, or add the symmetric `framework_entry_dirs()` skip to `dead-export` (see critical finding).
- **Status**: open

### minor — L estimate is reasonable, but Step 6 ambiguity makes it slip

- **File**: N/A
- **Description**: 7 steps covering schema extension, 3 new TOMLs, a small refactor, two wiring changes, and tests is consistent with an L (1-3 days). However if Option A in Step 6 is taken, that adds `Config` schema work plus serialization/loading plumbing — pushing toward XL. Resolving the Step 6 ambiguity is a prerequisite to trusting the L estimate.
- **Recommendation**: Pin Step 6 to Option B only; estimate stands.
- **Status**: open

### nit — `Route` magic-export name will leak across non-TanStack projects only if both deps coexist

- **File**: `/Users/rbaumier/www/comply/src/frameworks/tanstack-router.toml` (proposed)
- **Description**: Plan §Risks notes "`Route` magic export collision: gated by TanStack dep detection" — verified correct. `framework_magic_exports()` only yields names from detected frameworks, so non-TanStack projects are unaffected. No change.
- **Recommendation**: None.
- **Status**: open
