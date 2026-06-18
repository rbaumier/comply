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

// ── Shared helpers for the `in` / `typeof` boundary rules ─────────────────────

/** Source text of a node. */
function nodeText(text, sourceFile, node) {
  return text.slice(getTokenPosOfNode(node, sourceFile), node.end);
}

/** Peel parentheses, `as`/`satisfies`/`!` and property accesses down to the
 *  base identifier node (`(err as any).cause` → the `err` identifier). */
function baseIdentifierNode(node) {
  for (;;) {
    if (!node) return null;
    switch (node.kind) {
      case SyntaxKind.ParenthesizedExpression:
      case SyntaxKind.AsExpression:
      case SyntaxKind.SatisfiesExpression:
      case SyntaxKind.NonNullExpression:
      case SyntaxKind.PropertyAccessExpression:
        node = node.expression;
        continue;
      case SyntaxKind.Identifier:
        return node;
      default:
        return null;
    }
  }
}

/** Whether `operand` refers to the value bound by an enclosing `catch` — an
 *  `unknown`-by-nature value that `in`/`typeof` may legitimately inspect.
 *  Matched by symbol identity, not by name, so an inner binding that shadows
 *  the catch variable (`catch (e) { (e: T) => typeof e }`) is not mistaken for
 *  the caught value. A destructured binding (`catch ({ cause })`) has no single
 *  identifier symbol and is treated as not-a-caught-error. */
function operandIsCaughtError(checker, operand) {
  let clause = null;
  for (let p = operand.parent; p; p = p.parent) {
    if (p.kind === SyntaxKind.CatchClause) {
      clause = p;
      break;
    }
  }
  const binding = clause?.variableDeclaration?.name;
  if (!binding || binding.kind !== SyntaxKind.Identifier) return false;
  const id = baseIdentifierNode(operand);
  if (!id) return false;
  const catchSym = checker.getSymbolAtLocation(binding);
  return !!catchSym && catchSym === checker.getSymbolAtLocation(id);
}

/** Live runtime/DOM objects whose shape carries no methods yet must not be
 *  reconstructed from validated data (e.g. an element's `dataset`). */
const RUNTIME_OBJECT_TYPE_NAMES = new Set([
  "DOMStringMap",
  "DOMTokenList",
  "CSSStyleDeclaration",
  "NamedNodeMap",
]);

/** Whether a type owns a method (a property whose type is a function).
 *  The typescript-go API does not expose call-signature queries
 *  (`getCallSignatures` returns nothing on a constituent), so a function-typed
 *  property is detected from its rendered type containing `=>`. */
function typeHasMethod(checker, type) {
  const props = checker.getPropertiesOfType(type) || [];
  return props.some((p) => {
    const pt = checker.getTypeOfSymbol(p);
    return pt && /=>/.test(checker.typeToString(pt));
  });
}

/** Whether a value of this type cannot be reconstructed from validated data —
 *  it is a function, a JSX/runtime object, or a class instance with methods
 *  (Playwright `Page`/`Locator`, DOM nodes, …). For a union, true when any
 *  constituent is non-serializable (the `function`-variant discriminant case).
 *
 *  Method presence is the only available proxy for "live runtime object": the
 *  API exposes neither call signatures nor a symbol's declaration file, so a
 *  class instance from `node_modules` cannot be told apart from an owned object
 *  that happens to carry a method. The `in` rule therefore deliberately does
 *  NOT flag `key in ownedObjectWithAMethod` — favouring no false positive on
 *  the runtime objects this exemption exists for (DOM, Playwright). */
function isNonSerializable(checker, type) {
  return constituents(type).some((c) => {
    const s = checker.typeToString(c);
    const base = s.replace(/<.*$/s, "").trim();
    if (RUNTIME_OBJECT_TYPE_NAMES.has(base)) return true;
    if (/=>/.test(s)) return true;
    return typeHasMethod(checker, c);
  });
}

function pushDiag(sourceFile, lineStarts, file, node, rule, message) {
  const { line, column } = lineColAt(lineStarts, getTokenPosOfNode(node, sourceFile));
  diagnostics.push({ file, line, column, rule, message });
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

// ── Rule: ts-no-in-operator ──────────────────────────────────────────────────
// `"key" in obj` probes an object's shape by hand. External input must be parsed
// with a schema (Zod) and an owned union must be discriminated by a tag; the
// only legitimate `in` guards are on a caught error or a non-serializable
// runtime object (DOM dataset, Playwright Page vs Locator). `for ... in` is a
// ForInStatement (never a binary expression); `#field in obj` is the class-brand
// idiom and is skipped.
function ruleNoInOperator(sourceFile, checker, text, lineStarts, file) {
  walk(sourceFile, (node) => {
    if (node.kind !== SyntaxKind.BinaryExpression) return;
    if (node.operatorToken?.kind !== SyntaxKind.InKeyword) return;
    if (node.left?.kind === SyntaxKind.PrivateIdentifier) return;

    const rhs = node.right;
    if (operandIsCaughtError(checker, rhs)) return;
    const type = checker.getTypeAtLocation(rhs);
    if (type && isNonSerializable(checker, type)) return;

    pushDiag(
      sourceFile,
      lineStarts,
      file,
      node,
      "ts-no-in-operator",
      "Avoid `in` to probe shape: parse external input with a schema (e.g. Zod), or discriminate an owned union with a tag + exhaustive switch.",
    );
  });
}

// ── Rule: ts-no-typeof-operator ──────────────────────────────────────────────
// `typeof x` stands in for validating a boundary value or discriminating an
// owned union. It is allowed only as an environment guard (`typeof window`), on
// a caught error, inside a `z.preprocess` normaliser, or to discriminate a union
// whose variant is non-serializable (function/JSX, e.g. SetStateAction).
const ENV_GLOBALS = new Set([
  "window",
  "document",
  "globalThis",
  "self",
  "navigator",
  "process",
  "location",
  "localStorage",
  "sessionStorage",
  "WorkerGlobalScope",
  "Deno",
  "Bun",
  "require",
  "importScripts",
  "__DEV__",
]);

/** Whether `node` sits inside the callback argument of a `*.preprocess(...)`
 *  call (Zod's `z.preprocess(fn, schema)` normaliser). Scoped to the callback:
 *  a `typeof` in the schema argument, or merely anywhere under an unrelated
 *  `.preprocess(...)`, is not exempted. */
function inPreprocessCallback(text, sourceFile, node) {
  let fn = null;
  for (let p = node.parent; p; p = p.parent) {
    if (
      p.kind === SyntaxKind.ArrowFunction ||
      p.kind === SyntaxKind.FunctionExpression
    ) {
      fn = p;
      break;
    }
    if (
      p.kind === SyntaxKind.FunctionDeclaration ||
      p.kind === SyntaxKind.MethodDeclaration
    ) {
      return false;
    }
  }
  const call = fn?.parent;
  if (!call || call.kind !== SyntaxKind.CallExpression) return false;
  if (!(call.arguments || []).includes(fn)) return false;
  const callee = call.expression ? nodeText(text, sourceFile, call.expression).trim() : "";
  return /(^|\.)preprocess$/.test(callee);
}

function ruleNoTypeofOperator(sourceFile, checker, text, lineStarts, file) {
  walk(sourceFile, (node) => {
    if (node.kind !== SyntaxKind.TypeOfExpression) return;
    const operand = node.expression;

    if (
      operand.kind === SyntaxKind.Identifier &&
      ENV_GLOBALS.has(nodeText(text, sourceFile, operand))
    ) {
      return;
    }
    if (operandIsCaughtError(checker, operand)) return;
    if (inPreprocessCallback(text, sourceFile, node)) return;

    const type = checker.getTypeAtLocation(operand);
    if (type && type.flags & TypeFlags.Union && isNonSerializable(checker, type)) return;

    pushDiag(
      sourceFile,
      lineStarts,
      file,
      node,
      "ts-no-typeof-operator",
      "Avoid `typeof` here: parse external `unknown` with a schema (e.g. Zod), or discriminate an owned union with a tag + exhaustive switch.",
    );
  });
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
  if (enabled.has("ts-no-in-operator")) {
    ruleNoInOperator(sourceFile, checker, text, lineStarts, file);
  }
  if (enabled.has("ts-no-typeof-operator")) {
    ruleNoTypeofOperator(sourceFile, checker, text, lineStarts, file);
  }
}

if (enabled.has("no-duplicate-type-definition")) {
  emitDuplicateTypeDiagnostics(duplicateCandidates);
}

session.close?.();
process.stdout.write(JSON.stringify({ diagnostics }));
