#!/usr/bin/env bun
// Baseline perf playground for comply.
//
// Usage:   bun run tools/bench.ts
// Output:  tools/bench-baseline.json
//
// Measures wall-clock on progressively larger targets so each bottleneck
// shows up in isolation:
//   tiny-ts       -> startup + engine cold-path on 1 file
//   small-rules   -> a few Rust files, hits clippy subprocess
//   all-rules     -> ~500 Rust rule files, full clippy + ast-walk load
//   full-src      -> everything, what the user actually runs
//
// After optimization work, re-run and diff against bench-baseline.json.

import { spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdtempSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const BIN = join(ROOT, "target", "release", "comply");
const OUT = join(ROOT, "tools", "bench-baseline.json");

// Small synthetic TS fixture — isolates "startup + TS engine cold path"
// without depending on any real source file.
const TINY_TS_SRC = `
export function greet(name: string): string {
  const parts = ["hello", name];
  return parts.join(" ");
}

export class Counter {
  private n = 0;
  increment(): number {
    this.n += 1;
    return this.n;
  }
}

const items = [1, 2, 3, 4, 5];
const doubled = items.map((x) => x * 2);
const sum = items.reduce((a, b) => a + b, 0);
console.log(greet("world"), doubled, sum);
`;

type Target = {
  name: string;
  /** Path to lint, relative to ROOT or absolute (for the tempdir fixture). */
  path: string;
  description: string;
};

function buildTargets(fixtureDir: string): Target[] {
  return [
    {
      name: "tiny-ts",
      path: join(fixtureDir, "tiny.ts"),
      description: "synthetic 1 TS file (startup + cold engine)",
    },
    {
      name: "small-rules",
      path: "src/rules/no_array_reduce",
      description: "one rule dir (~2 Rust files)",
    },
    {
      name: "many-rules",
      path: join(fixtureDir, "many-rules"),
      description: `${HUNDRED_RULES_COUNT} rule dirs copied to tempdir`,
    },
  ];
}

// How many rule dirs to include in the "many-rules" target.
const HUNDRED_RULES_COUNT = 100;

// Directory names under src/rules/ that are NOT rule modules (shared
// helpers, backend infra, delegated-upstream glue). Excluded from the
// auto-picked list so the bench sees real rule workloads.
const RULES_EXCLUDE = new Set([
  "backend.rs",
  "delegated",
  "jsx.rs",
  "meta.rs",
  "mod.rs",
  "registry.rs",
  "rust_helpers.rs",
  "test_helpers.rs",
  "vue_template_helpers.rs",
  "walker.rs",
]);

function pickRuleDirs(rulesRoot: string, count: number): string[] {
  const entries = readdirSync(rulesRoot, { withFileTypes: true });
  const picked: string[] = [];
  for (const e of entries) {
    if (!e.isDirectory()) continue;
    if (RULES_EXCLUDE.has(e.name)) continue;
    // Must have a mod.rs to count as a real rule dir.
    if (!existsSync(join(rulesRoot, e.name, "mod.rs"))) continue;
    picked.push(e.name);
    if (picked.length >= count) break;
  }
  return picked;
}

const WARMUP = 1;
const RUNS = 5;

// Phase labels emitted by `comply --timings` (src/main.rs print_timings).
// Order is significant — it's the display order in the per-target breakdown.
const PHASES = [
  "discovery",
  "config",
  "fix",
  "oxlint",
  "jscpd (ts)",
  "knip",
  "madge",
  "engine (ts)",
  "clippy",
  "cargo-shear",
  "cargo-modules",
  "jscpd (rs)",
  "engine (rs)",
  "engine (vue)",
  "post-filter",
  "TOTAL",
] as const;
type Phase = (typeof PHASES)[number];

const SRC_EXTS = new Set([
  ".ts",
  ".tsx",
  ".js",
  ".jsx",
  ".rs",
  ".vue",
]);

function countFiles(absPath: string): number {
  const s = statSync(absPath);
  if (s.isFile()) {
    const dot = absPath.lastIndexOf(".");
    return dot >= 0 && SRC_EXTS.has(absPath.slice(dot)) ? 1 : 0;
  }
  let n = 0;
  for (const entry of readdirSync(absPath, { withFileTypes: true })) {
    if (
      entry.name === "target" ||
      entry.name === "node_modules" ||
      entry.name.startsWith(".")
    ) {
      continue;
    }
    n += countFiles(join(absPath, entry.name));
  }
  return n;
}

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = Math.min(
    sorted.length - 1,
    Math.floor((sorted.length * p) / 100),
  );
  return sorted[idx]!;
}

function stddev(xs: number[]): number {
  const m = xs.reduce((a, b) => a + b, 0) / xs.length;
  const v = xs.reduce((a, b) => a + (b - m) ** 2, 0) / xs.length;
  return Math.sqrt(v);
}

function fmtMs(ms: number): string {
  if (ms >= 1000) return `${(ms / 1000).toFixed(2)}s`;
  return `${ms.toFixed(1)}ms`;
}

function pad(s: string | number, w: number, right = false): string {
  const str = String(s);
  if (str.length >= w) return str;
  const filler = " ".repeat(w - str.length);
  return right ? filler + str : str + filler;
}

type RunSample = {
  wallMs: number;
  phases: Partial<Record<Phase, number>>;
};

function parseTimings(stderr: string): Partial<Record<Phase, number>> {
  const out: Partial<Record<Phase, number>> = {};
  for (const line of stderr.split("\n")) {
    // Match lines like "  clippy        1234.5ms" or "  TOTAL          42.3ms".
    // The header "-- typescript --", the "-----" separator, and "comply:
    // timings breakdown" never match because they have no "<number>ms" tail.
    const m = line.match(/^\s{2,}([A-Za-z][A-Za-z\-\s()]*?)\s{2,}([\d.]+)ms\s*$/);
    if (!m) continue;
    const label = m[1]!.trim();
    const ms = Number.parseFloat(m[2]!);
    if (!Number.isFinite(ms)) continue;
    if ((PHASES as readonly string[]).includes(label)) {
      out[label as Phase] = ms;
    }
  }
  return out;
}

function runOnce(targetPath: string, withTimings: boolean): RunSample {
  const args = [targetPath, "--json"];
  if (withTimings) args.push("--timings");
  const start = performance.now();
  const res = spawnSync(BIN, args, {
    cwd: ROOT,
    stdio: ["ignore", "pipe", "pipe"],
  });
  const wallMs = performance.now() - start;
  // comply exits 0 (clean) or 1 (violations found) — both are fine.
  // Exit code 2 = crash, anything else = subprocess problem we want to see.
  if (res.status !== 0 && res.status !== 1) {
    const stderr = res.stderr?.toString() ?? "";
    throw new Error(
      `comply exited with status ${res.status} on ${targetPath}\n${stderr}`,
    );
  }
  const phases = withTimings
    ? parseTimings(res.stderr?.toString() ?? "")
    : {};
  return { wallMs, phases };
}

function build(): void {
  process.stdout.write("building comply --release ... ");
  const t0 = performance.now();
  const res = spawnSync("cargo", ["build", "--release", "--quiet"], {
    cwd: ROOT,
    stdio: ["ignore", "inherit", "inherit"],
  });
  if (res.status !== 0) {
    console.error("\ncargo build failed");
    process.exit(1);
  }
  console.log(`done in ${fmtMs(performance.now() - t0)}`);
}

type PhaseStats = {
  median: number;
  pctOfTotal: number;
};

type Result = {
  name: string;
  path: string;
  description: string;
  fileCount: number;
  wallSamples: number[];
  phaseSamples: Partial<Record<Phase, number[]>>;
  stats: {
    min: number;
    median: number;
    mean: number;
    p95: number;
    max: number;
    stddev: number;
    filesPerSec: number;
  };
  phaseMedians: Partial<Record<Phase, PhaseStats>>;
};

function medianOf(xs: number[]): number {
  if (xs.length === 0) return 0;
  const sorted = [...xs].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)]!;
}

function bench(target: Target): Result | null {
  const abs = isAbsolute(target.path) ? target.path : join(ROOT, target.path);
  if (!existsSync(abs)) {
    console.log(`[skip] ${target.name}: ${target.path} not found`);
    return null;
  }

  const fileCount = countFiles(abs);
  console.log(
    `\n[${target.name}]  ${target.description}  (${fileCount} source files)`,
  );

  // Warmup: let clippy, cargo metadata, and filesystem caches prime.
  for (let i = 0; i < WARMUP; i++) {
    runOnce(target.path, false);
  }

  const wallSamples: number[] = [];
  const phaseSamples: Partial<Record<Phase, number[]>> = {};

  // All measured runs use --timings so we get wall-clock AND decomposition
  // in one pass. --timings overhead is negligible (Instant::now() ~10ns).
  for (let i = 0; i < RUNS; i++) {
    const sample = runOnce(target.path, true);
    wallSamples.push(sample.wallMs);
    for (const [phase, ms] of Object.entries(sample.phases)) {
      const key = phase as Phase;
      (phaseSamples[key] ??= []).push(ms);
    }
    console.log(`  run ${pad(i + 1, 2, true)}/${RUNS}  ${fmtMs(sample.wallMs)}`);
  }

  const sorted = [...wallSamples].sort((a, b) => a - b);
  const min = sorted[0]!;
  const max = sorted[sorted.length - 1]!;
  const median = percentile(sorted, 50);
  const p95 = percentile(sorted, 95);
  const mean = wallSamples.reduce((a, b) => a + b, 0) / wallSamples.length;
  const sd = stddev(wallSamples);
  const filesPerSec = median > 0 ? (fileCount * 1000) / median : 0;

  console.log(
    `  wall: min=${fmtMs(min)}  median=${fmtMs(median)}  mean=${fmtMs(mean)}  p95=${fmtMs(p95)}  max=${fmtMs(max)}  stddev=${fmtMs(sd)}  files/s=${filesPerSec.toFixed(0)}`,
  );

  const totalPhaseMedian = medianOf(phaseSamples.TOTAL ?? []);
  const phaseMedians: Partial<Record<Phase, PhaseStats>> = {};
  console.log("  phase breakdown (median across runs):");
  for (const phase of PHASES) {
    const arr = phaseSamples[phase] ?? [];
    if (arr.length === 0) continue;
    const phaseMedian = medianOf(arr);
    // Skip zero-cost phases in the printed output to keep it scannable.
    if (phaseMedian < 0.1 && phase !== "TOTAL") continue;
    const pct =
      totalPhaseMedian > 0 && phase !== "TOTAL"
        ? (phaseMedian / totalPhaseMedian) * 100
        : 0;
    phaseMedians[phase] = { median: phaseMedian, pctOfTotal: pct };
    const pctStr = phase === "TOTAL" ? "      " : `${pct.toFixed(1).padStart(5)}%`;
    console.log(
      `    ${pad(phase, 15)}  ${pad(fmtMs(phaseMedian), 10, true)}   ${pctStr}`,
    );
  }

  return {
    name: target.name,
    path: target.path,
    description: target.description,
    fileCount,
    wallSamples,
    phaseSamples,
    stats: { min, median, mean, p95, max, stddev: sd, filesPerSec },
    phaseMedians,
  };
}

function dominantPhase(r: Result): string {
  let best: { phase: string; pct: number } | null = null;
  for (const [phase, stats] of Object.entries(r.phaseMedians)) {
    if (phase === "TOTAL" || !stats) continue;
    if (!best || stats.pctOfTotal > best.pct) {
      best = { phase, pct: stats.pctOfTotal };
    }
  }
  return best ? `${best.phase} (${best.pct.toFixed(0)}%)` : "n/a";
}

function summary(results: Result[]): void {
  const line = "-".repeat(96);
  console.log(`\n${line}`);
  console.log("SUMMARY");
  console.log(line);
  console.log(
    `${pad("target", 14)}${pad("files", 8, true)}${pad("median", 12, true)}${pad("p95", 12, true)}${pad("files/s", 10, true)}   ${pad("dominant phase", 30)}`,
  );
  for (const r of results) {
    console.log(
      `${pad(r.name, 14)}${pad(r.fileCount, 8, true)}${pad(fmtMs(r.stats.median), 12, true)}${pad(fmtMs(r.stats.p95), 12, true)}${pad(r.stats.filesPerSec.toFixed(0), 10, true)}   ${pad(dominantPhase(r), 30)}`,
    );
  }
  console.log(line);
}

function gitInfo(): { commit: string; dirty: boolean } {
  const commit = spawnSync("git", ["rev-parse", "HEAD"], { cwd: ROOT })
    .stdout.toString()
    .trim();
  const status = spawnSync("git", ["status", "--porcelain"], { cwd: ROOT })
    .stdout.toString()
    .trim();
  return { commit, dirty: status.length > 0 };
}

function createFixtureDir(): string {
  const dir = mkdtempSync(join(tmpdir(), "comply-bench-"));
  writeFileSync(join(dir, "tiny.ts"), TINY_TS_SRC);

  const rulesRoot = join(ROOT, "src", "rules");
  const picked = pickRuleDirs(rulesRoot, HUNDRED_RULES_COUNT);
  console.log(
    `fixture: picked ${picked.length} rule dirs from src/rules/ for many-rules target`,
  );
  const manyDir = join(dir, "many-rules");
  for (const rule of picked) {
    cpSync(join(rulesRoot, rule), join(manyDir, rule), { recursive: true });
  }
  return dir;
}

function main(): void {
  console.log(`comply bench playground  ${new Date().toISOString()}`);
  console.log(`root: ${ROOT}`);
  console.log(`runs: ${RUNS}  warmup: ${WARMUP}`);

  build();

  const fixtureDir = createFixtureDir();
  const targets = buildTargets(fixtureDir);

  const results: Result[] = [];
  try {
    for (const t of targets) {
      const r = bench(t);
      if (r) results.push(r);
    }
  } finally {
    rmSync(fixtureDir, { recursive: true, force: true });
  }

  summary(results);

  const git = gitInfo();
  const payload = {
    timestamp: new Date().toISOString(),
    commit: git.commit,
    dirty: git.dirty,
    runs: RUNS,
    warmup: WARMUP,
    results,
  };
  writeFileSync(OUT, `${JSON.stringify(payload, null, 2)}\n`);
  console.log(`\nsaved baseline -> ${OUT}`);
  if (git.dirty) {
    console.log(
      "warning: working tree dirty, baseline reflects uncommitted code",
    );
  }
}

main();
