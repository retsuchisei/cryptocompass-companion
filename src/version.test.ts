import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { disagreements, versionInCargoToml } from "./version.ts";

const root = new URL("../", import.meta.url);

function manifest(name: string): string {
  return readFileSync(new URL(name, root), "utf8");
}

test("a disagreement names the manifest that disagrees", () => {
  assert.deepEqual(
    disagreements([
      { where: "package.json", version: "0.1.0" },
      { where: "tauri.conf.json", version: "0.1.0" },
    ]),
    [],
  );

  assert.deepEqual(
    disagreements([
      { where: "package.json", version: "0.2.0" },
      { where: "tauri.conf.json", version: "0.2.0" },
      { where: "Cargo.toml", version: "0.1.0" },
    ]),
    ["Cargo.toml"],
    "the first declaration is the one the others are read against",
  );
});

test("the crate's own version is the one under [package]", () => {
  assert.equal(
    versionInCargoToml(
      [
        "[package]",
        'name = "cryptocompass-companion"',
        'version = "0.3.1"',
        "",
        "[dependencies]",
        'tauri = { version = "2", features = [] }',
      ].join("\n"),
    ),
    "0.3.1",
    "a dependency's version is not the crate's",
  );

  assert.equal(versionInCargoToml("[dependencies]\ntauri = \"2\"\n"), null);
});

// The tag names the release, tauri.conf.json's version is what the updater
// reads out of latest.json, and Cargo's is what consent is compared against -
// so a bump that reaches two of the three ships an update that is either never
// offered or never asks again. Neither failure is visible from the outside.
test("one version, declared in three places", () => {
  const cargo = versionInCargoToml(manifest("src-tauri/Cargo.toml"));
  assert.ok(cargo, "the crate declares no version");

  const declared = [
    { where: "package.json", version: JSON.parse(manifest("package.json")).version },
    {
      where: "src-tauri/tauri.conf.json",
      version: JSON.parse(manifest("src-tauri/tauri.conf.json")).version,
    },
    { where: "src-tauri/Cargo.toml", version: cargo },
  ];

  assert.deepEqual(disagreements(declared), [], `not on ${declared[0].version}`);
});
