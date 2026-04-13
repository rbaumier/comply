#!/usr/bin/env bun
// Codemod: inject `span: None,` into every `Diagnostic { ... }` struct literal
// that doesn't already have it.
//
// Why a codemod and not sed: rule messages embed `{` / `}` inside string
// literals, so brace-balancing is required. This walks the file with a minimal
// Rust lexer that tracks string, char, line-comment, and block-comment state
// so matches inside comments or strings are ignored.
//
// Idempotent: rerunning it is a no-op because literals with an existing
// `span:` field are left untouched.
import { readFileSync, writeFileSync } from "node:fs";
import { $ } from "bun";

type State =
  | "code"
  | "line_comment"
  | "block_comment"
  | "string"
  | "raw_string"
  | "char";

function isIdent(c: string): boolean {
  return /[A-Za-z0-9_]/.test(c);
}

// Walk the source char-by-char, tracking lexical state. At each top-level
// occurrence of `Diagnostic {` (word-boundary, in code context, preceded by
// something other than `struct` / `->`), find the matching closing `}` using
// the same state tracker, then yield the match.
interface Match {
  literalStart: number; // offset of `Diagnostic`
  bodyStart: number; // offset just after `{`
  bodyEnd: number; // offset of matching `}` (exclusive)
}

function findDiagnosticLiterals(src: string): Match[] {
  const matches: Match[] = [];
  let state: State = "code";
  let escape = false;
  let rawHashes = 0;
  let i = 0;

  while (i < src.length) {
    const c = src[i];
    const next = src[i + 1];

    if (escape) {
      escape = false;
      i++;
      continue;
    }

    if (state === "line_comment") {
      if (c === "\n") state = "code";
      i++;
      continue;
    }

    if (state === "block_comment") {
      if (c === "*" && next === "/") {
        state = "code";
        i += 2;
        continue;
      }
      i++;
      continue;
    }

    if (state === "string") {
      if (c === "\\") {
        escape = true;
        i++;
        continue;
      }
      if (c === '"') {
        state = "code";
        i++;
        continue;
      }
      i++;
      continue;
    }

    if (state === "raw_string") {
      // Raw strings end at `"` followed by the same number of `#` that
      // opened them (stored in `rawHashes`).
      if (c === '"') {
        let ok = true;
        for (let h = 0; h < rawHashes; h++) {
          if (src[i + 1 + h] !== "#") { ok = false; break; }
        }
        if (ok) {
          state = "code";
          i += 1 + rawHashes;
          rawHashes = 0;
          continue;
        }
      }
      i++;
      continue;
    }

    if (state === "char") {
      if (c === "\\") {
        escape = true;
        i++;
        continue;
      }
      if (c === "'") {
        state = "code";
        i++;
        continue;
      }
      i++;
      continue;
    }

    // state === "code"
    if (c === "/" && next === "/") {
      state = "line_comment";
      i += 2;
      continue;
    }
    if (c === "/" && next === "*") {
      state = "block_comment";
      i += 2;
      continue;
    }
    if (c === '"') {
      state = "string";
      i++;
      continue;
    }
    // Raw string `r"..."` or `r#"..."#` or `r##"..."##`, etc.
    if (c === "r" && !isIdent(src[i - 1] ?? "")) {
      let h = 0;
      while (src[i + 1 + h] === "#") h++;
      if (src[i + 1 + h] === '"') {
        state = "raw_string";
        rawHashes = h;
        i += 2 + h;
        continue;
      }
    }
    if (c === "'") {
      // Distinguish char literal from lifetime. Char literal has the form
      // `'.'` or `'\x'`. Lifetime is `'ident` followed by non-`'`.
      const p2 = src[i + 2];
      const p3 = src[i + 3];
      if (next === "\\" || p2 === "'" || p3 === "'") {
        state = "char";
        i++;
        continue;
      }
      // Lifetime — consume identifier chars.
      i++;
      while (i < src.length && isIdent(src[i])) i++;
      continue;
    }

    // Check for `Diagnostic {` / `ComplyDiagnostic {` with word boundary.
    // `ComplyDiagnostic` is a local alias used in `src/lsp.rs`
    // (`use crate::diagnostic::Diagnostic as ComplyDiagnostic`).
    let token: string | null = null;
    if (src.startsWith("ComplyDiagnostic {", i)) token = "ComplyDiagnostic {";
    else if (c === "D" && src.startsWith("Diagnostic {", i))
      token = "Diagnostic {";
    if (token) {
      const charBefore = i > 0 ? src[i - 1] : "";
      if (!isIdent(charBefore)) {
        // Look at preceding non-whitespace for struct/return-type exclusion.
        let k = i - 1;
        while (k >= 0 && /\s/.test(src[k])) k--;
        const precedingChar = k >= 0 ? src[k] : "";
        const wordEnd = k + 1;
        let w = k;
        while (w >= 0 && /[A-Za-z_]/.test(src[w])) w--;
        const precedingWord = src.slice(w + 1, wordEnd);

        if (precedingWord !== "struct" && precedingChar !== ">") {
          // Literal context. Walk to matching closing brace.
          const literalStart = i;
          const bodyStart = i + token.length;
          const end = findMatchingBrace(src, bodyStart);
          if (end < 0) {
            throw new Error(
              `Unbalanced ${token.trim()} literal at offset ${literalStart}`,
            );
          }
          matches.push({ literalStart, bodyStart, bodyEnd: end });
          // Continue scanning after the closing brace.
          i = end + 1;
          continue;
        }
      }
    }

    i++;
  }

  return matches;
}

// Walk from `start` (just after an opening `{`) to the matching `}`. Tracks
// nested braces and ignores braces inside strings/comments. Returns the
// offset of the matching `}`, or -1 if unbalanced.
function findMatchingBrace(src: string, start: number): number {
  let depth = 1;
  let state: State = "code";
  let escape = false;
  let rawHashes = 0;
  let i = start;

  while (i < src.length) {
    const c = src[i];
    const next = src[i + 1];

    if (escape) {
      escape = false;
      i++;
      continue;
    }

    if (state === "line_comment") {
      if (c === "\n") state = "code";
      i++;
      continue;
    }

    if (state === "block_comment") {
      if (c === "*" && next === "/") {
        state = "code";
        i += 2;
        continue;
      }
      i++;
      continue;
    }

    if (state === "string") {
      if (c === "\\") {
        escape = true;
        i++;
        continue;
      }
      if (c === '"') {
        state = "code";
        i++;
        continue;
      }
      i++;
      continue;
    }

    if (state === "raw_string") {
      if (c === '"') {
        state = "code";
        i++;
        continue;
      }
      i++;
      continue;
    }

    if (state === "char") {
      if (c === "\\") {
        escape = true;
        i++;
        continue;
      }
      if (c === "'") {
        state = "code";
        i++;
        continue;
      }
      i++;
      continue;
    }

    // code
    if (c === "/" && next === "/") {
      state = "line_comment";
      i += 2;
      continue;
    }
    if (c === "/" && next === "*") {
      state = "block_comment";
      i += 2;
      continue;
    }
    if (c === '"') {
      state = "string";
      i++;
      continue;
    }
    if (c === "r" && next === '"' && !isIdent(src[i - 1] ?? "")) {
      state = "raw_string";
      i += 2;
      continue;
    }
    if (c === "'") {
      const p2 = src[i + 2];
      const p3 = src[i + 3];
      if (next === "\\" || p2 === "'" || p3 === "'") {
        state = "char";
        i++;
        continue;
      }
      i++;
      while (i < src.length && isIdent(src[i])) i++;
      continue;
    }
    if (c === "{") {
      depth++;
      i++;
      continue;
    }
    if (c === "}") {
      depth--;
      if (depth === 0) return i;
      i++;
      continue;
    }
    i++;
  }

  return -1;
}

const files = (await $`git ls-files src --full-name`.text())
  .trim()
  .split("\n")
  .filter((f) => f.endsWith(".rs"));

let edited = 0;
let literals = 0;
let filesChanged = 0;

for (const f of files) {
  const src = readFileSync(f, "utf8");
  if (!src.includes("Diagnostic {")) continue;

  const matches = findDiagnosticLiterals(src);
  if (matches.length === 0) continue;

  // Build the patched file by walking matches in order.
  const parts: string[] = [];
  let cursor = 0;
  let fileEdited = false;

  for (const m of matches) {
    literals++;
    const body = src.slice(m.bodyStart, m.bodyEnd);

    // Copy everything up to and including `Diagnostic {`.
    parts.push(src.slice(cursor, m.bodyStart));

    if (/\bspan\s*:/.test(body)) {
      // Already has span.
      parts.push(body);
      cursor = m.bodyEnd;
      continue;
    }

    // Determine indentation from the last non-empty line before the closing `}`.
    const lines = body.split("\n");
    let indent = "            ";
    for (let l = lines.length - 1; l >= 0; l--) {
      const line = lines[l];
      if (line.trim().length > 0) {
        const match = line.match(/^(\s*)/);
        if (match) indent = match[1];
        break;
      }
    }

    // Separate trailing whitespace (typically `\n<indent>`) so the closing
    // `}` stays on its own line.
    const trimmedEnd = body.replace(/\s*$/, "");
    const trailingWhitespace = body.slice(trimmedEnd.length);
    const needsComma = !trimmedEnd.endsWith(",");
    const insertion = `${needsComma ? "," : ""}\n${indent}span: None,${trailingWhitespace}`;

    parts.push(trimmedEnd);
    parts.push(insertion);
    cursor = m.bodyEnd;
    edited++;
    fileEdited = true;
  }

  parts.push(src.slice(cursor));
  const patched = parts.join("");
  if (patched !== src) {
    writeFileSync(f, patched);
    if (fileEdited) filesChanged++;
  }
}

console.log(
  `Patched ${edited}/${literals} Diagnostic literals across ${filesChanged}/${files.length} files`,
);
