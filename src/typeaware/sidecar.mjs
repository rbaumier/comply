// comply type-aware sidecar.
//
// Driven by comply's Rust process when `--type-aware` is set. Reads a single
// JSON request on stdin, builds the TypeScript program once via typescript-go
// (@typescript/native-preview), runs the enabled type-aware rules against it,
// and writes a single JSON response on stdout.
//
// Request:  { "tsconfig": string, "files": string[], "rules": string[] }
// Response: { "diagnostics": Diag[], "error"?: string }
//   Diag:   { "file": string, "line": number, "column": number,
//             "rule": string, "message": string }
//
// The @typescript/native-preview package is resolved from the linted project's
// node_modules (comply spawns this script with cwd = project root); a missing
// package yields { error: "package-not-found" } so comply can print an install
// hint and skip the phase gracefully.

import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";
import fs from "node:fs";
import path from "node:path";

function fail(error) {
  process.stdout.write(JSON.stringify({ diagnostics: [], error }));
  process.exit(0);
}

async function readStdin() {
  const chunks = [];
  for await (const chunk of process.stdin) chunks.push(chunk);
  return Buffer.concat(chunks).toString("utf8");
}

const req = JSON.parse(await readStdin());
const { tsconfig, files = [], rules = [] } = req;
const enabled = new Set(rules);

// Resolve the typescript-go API from the project's node_modules.
const require = createRequire(path.join(process.cwd(), "comply-typeaware-resolver.js"));
let apiMod, ast, TypeFlags;
try {
  const apiPath = require.resolve("@typescript/native-preview/unstable/sync");
  const astPath = require.resolve("@typescript/native-preview/unstable/ast");
  apiMod = await import(pathToFileURL(apiPath).href);
  ast = await import(pathToFileURL(astPath).href);
  TypeFlags = apiMod.TypeFlags;
} catch {
  fail("package-not-found");
}

const { API } = apiMod;
const { SyntaxKind, computeLineStarts, getTokenPosOfNode } = ast;

let session;
try {
  session = new API({ cwd: path.dirname(tsconfig) });
} catch (e) {
  fail(`api-init-failed: ${e?.message ?? e}`);
}

let snapshot;
try {
  snapshot = session.updateSnapshot({ openProject: tsconfig });
} catch (e) {
  fail(`snapshot-failed: ${e?.message ?? e}`);
}

const diagnostics = [];

/** Walk every node depth-first, calling `visit` on each. */
function walk(node, visit) {
  if (!node) return;
  visit(node);
  node.forEachChild((child) => walk(child, visit));
}

/** Map a 0-based char offset to a 1-based { line, column } via line starts. */
function lineColAt(lineStarts, pos) {
  let lo = 0;
  let hi = lineStarts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (lineStarts[mid] <= pos) lo = mid;
    else hi = mid - 1;
  }
  return { line: lo + 1, column: pos - lineStarts[lo] + 1 };
}

/** Constituents of a union type, or the type itself when not a union. */
function constituents(type) {
  return type.flags & TypeFlags.Union && typeof type.getTypes === "function"
    ? type.getTypes()
    : [type];
}

function nullishMembership(type) {
  const cs = constituents(type);
  return {
    hasNull: cs.some((c) => c.flags & TypeFlags.Null),
    hasUndefined: cs.some((c) => c.flags & TypeFlags.Undefined),
  };
}

// ── Rule: no-redundant-nullish-coalescing-null ───────────────────────────────
// `x ?? null` is a no-op when x's type already includes `null` and not
// `undefined` (the coalesce can never change the value or the type). Symmetric
// for `x ?? undefined`. A `??` whose left side mixes both null and undefined,
// or whose right side isn't the matching literal, is left alone.
function ruleRedundantNullishCoalescing(sourceFile, checker, text, lineStarts, file) {
  walk(sourceFile, (node) => {
    if (node.kind !== SyntaxKind.BinaryExpression) return;
    if (node.operatorToken?.kind !== SyntaxKind.QuestionQuestionToken) return;
    const right = node.right;
    const rightText = text.slice(getTokenPosOfNode(right, sourceFile), right.end).trim();
    const rhsIsNull = right.kind === SyntaxKind.NullKeyword;
    const rhsIsUndefined =
      right.kind === SyntaxKind.Identifier && rightText === "undefined";
    if (!rhsIsNull && !rhsIsUndefined) return;

    const lhsType = checker.getTypeAtLocation(node.left);
    if (!lhsType) return;
    const { hasNull, hasUndefined } = nullishMembership(lhsType);

    const redundant = rhsIsNull
      ? hasNull && !hasUndefined
      : hasUndefined && !hasNull;
    if (!redundant) return;

    const start = getTokenPosOfNode(node, sourceFile);
    const { line, column } = lineColAt(lineStarts, start);
    diagnostics.push({
      file,
      line,
      column,
      rule: "no-redundant-nullish-coalescing-null",
      message: `\`?? ${rhsIsNull ? "null" : "undefined"}\` is redundant — the left operand's type already includes ${rhsIsNull ? "null" : "undefined"}.`,
    });
  });
}

// ── Rule: no-duplicate-type-definition ───────────────────────────────────────
// Two or more named types (across the whole program) whose object shape is
// structurally identical — a copy-paste smell. Conservative to avoid flagging
// intentionally-distinct shapes (branded types, DTO vs domain): only object
// shapes (`interface` or `type = { … }`), only when the shape has at least 3
// properties, reported as a warning. The fingerprint is built from the
// resolved property types, not the alias name, so different names with the
// same shape collide.
const MIN_DUP_PROPERTIES = 3;

function structuralFingerprint(checker, type) {
  const props = checker.getPropertiesOfType(type) || [];
  if (props.length < MIN_DUP_PROPERTIES) return null;
  const parts = props.map((p) => {
    const t = checker.getTypeOfSymbol(p);
    return `${p.name}:${t ? checker.typeToString(t) : "?"}`;
  });
  parts.sort();
  return parts.join(";");
}

/** Collect duplicate-type candidates from one file into `acc`. */
function collectDuplicateTypeCandidates(sourceFile, checker, text, lineStarts, file, acc) {
  const slice = (n) => text.slice(getTokenPosOfNode(n, sourceFile), n.end);
  walk(sourceFile, (node) => {
    // Only object shapes: an interface, or a `type X = { … }` type literal.
    const isObjectShape =
      node.kind === SyntaxKind.InterfaceDeclaration ||
      (node.kind === SyntaxKind.TypeAliasDeclaration &&
        node.type?.kind === SyntaxKind.TypeLiteral);
    if (!isObjectShape) return;
    const sym = checker.getSymbolAtLocation(node.name);
    if (!sym) return;
    const type = checker.getDeclaredTypeOfSymbol(sym);
    const fingerprint = type ? structuralFingerprint(checker, type) : null;
    if (!fingerprint) return;
    const start = getTokenPosOfNode(node.name, sourceFile);
    const { line, column } = lineColAt(lineStarts, start);
    acc.push({ file, name: slice(node.name), line, column, fingerprint });
  });
}

/** Group collected candidates by fingerprint and emit a diagnostic per member
 *  of any group with two or more distinct declarations. */
function emitDuplicateTypeDiagnostics(candidates) {
  const groups = new Map();
  for (const c of candidates) {
    if (!groups.has(c.fingerprint)) groups.set(c.fingerprint, []);
    groups.get(c.fingerprint).push(c);
  }
  for (const members of groups.values()) {
    if (members.length < 2) continue;
    for (const m of members) {
      const others = members
        .filter((o) => o !== m)
        .map((o) => `\`${o.name}\``)
        .join(", ");
      diagnostics.push({
        file: m.file,
        line: m.line,
        column: m.column,
        rule: "no-duplicate-type-definition",
        message: `Type \`${m.name}\` is structurally identical to ${others} — consolidate into one type.`,
      });
    }
  }
}

const duplicateCandidates = [];

for (const file of files) {
  const project = snapshot.getDefaultProjectForFile(file);
  if (!project) continue;
  const sourceFile = project.program.getSourceFile(file);
  if (!sourceFile) continue;
  const checker = project.checker;
  let text;
  try {
    text = fs.readFileSync(file, "utf8");
  } catch {
    continue;
  }
  const lineStarts = computeLineStarts(text);

  if (enabled.has("no-redundant-nullish-coalescing-null")) {
    ruleRedundantNullishCoalescing(sourceFile, checker, text, lineStarts, file);
  }
  if (enabled.has("no-duplicate-type-definition")) {
    collectDuplicateTypeCandidates(sourceFile, checker, text, lineStarts, file, duplicateCandidates);
  }
}

if (enabled.has("no-duplicate-type-definition")) {
  emitDuplicateTypeDiagnostics(duplicateCandidates);
}

session.close?.();
process.stdout.write(JSON.stringify({ diagnostics }));
