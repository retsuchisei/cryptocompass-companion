#!/usr/bin/env node
/**
 * The language rule, made checkable.
 *
 * This app is ASCII-only except for its locale catalogues. Everything else -
 * code, comments, errors - is English, so a file can be read by anyone who
 * works on it and a user-facing string has exactly one home.
 *
 * The rule exists because the sibling guidebook did the opposite: its Russian
 * is written where it is displayed, and it has been stuck without an i18n
 * layer ever since. Catching the first stray string costs nothing; retrofitting
 * a catalogue over a finished interface costs a rewrite.
 */
import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";

// The catalogues, and nothing else.
const CATALOGUES = /^src\/i18n\//;

const SKIP =
  /\.(png|jpe?g|webp|gif|ico|icns|svg|woff2?)$|^package-lock\.json$|^src-tauri\/(target|gen|icons)\//;

const files = execFileSync("git", ["ls-files", "-c", "-o", "--exclude-standard"], {
  encoding: "utf8",
})
  .split("\n")
  .filter(Boolean)
  .filter((file) => !SKIP.test(file));

let bad = 0;

for (const file of files) {
  let text;

  try {
    if (statSync(file).isDirectory()) continue;
    text = readFileSync(file, "utf8");
  } catch {
    continue;
  }

  if (text.charCodeAt(0) === 0xfeff) {
    console.error(`${file}:1  byte order mark`);
    bad += 1;
  }

  text.split("\n").forEach((line, index) => {
    if (![...line].some((ch) => ch.charCodeAt(0) > 127)) return;

    // A catalogue may hold user-facing text; it still may not hold a comment
    // in another language.
    if (CATALOGUES.test(file)) {
      if (/^\s*(\/\/|\*|\/\*)/.test(line)) {
        console.error(`${file}:${index + 1}  non-ASCII comment: ${line.trim().slice(0, 60)}`);
        bad += 1;
      }
      return;
    }

    console.error(`${file}:${index + 1}  non-ASCII outside src/i18n/: ${line.trim().slice(0, 60)}`);
    bad += 1;
  });
}

if (bad) {
  console.error(`\nASCII check failed: ${bad} line(s). User-facing text belongs in src/i18n/.`);
  process.exit(1);
}

console.log(`ASCII check passed (${files.length} files)`);
