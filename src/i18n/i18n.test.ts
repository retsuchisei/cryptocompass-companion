import { test, describe } from "node:test";
import assert from "node:assert/strict";

import { en } from "./en.ts";
import { ru } from "./ru.ts";
import { isLocale, preferred } from "./index.ts";

/**
 * The catalogues are checked against each other rather than by eye. A key that
 * reaches only one of them is invisible until a reader in the other language
 * opens the screen it is on - and the first screen this app shows is the
 * consent screen, which is the worst possible place to find a blank.
 */
describe("the catalogues", () => {
  test("hold the same keys", () => {
    const missing = Object.keys(en).filter((key) => !(key in ru));
    const extra = Object.keys(ru).filter((key) => !(key in en));

    assert.deepEqual(missing, [], "ru is missing keys that en has");
    assert.deepEqual(extra, [], "ru has keys en does not");
  });

  test("agree on which keys take arguments", () => {
    for (const [key, value] of Object.entries(en)) {
      const other = (ru as Record<string, unknown>)[key];
      assert.equal(
        typeof other,
        typeof value,
        `${key} is a ${typeof value} in en and a ${typeof other} in ru`,
      );
    }
  });

  test("say something in both languages, never an empty string", () => {
    for (const table of [en, ru]) {
      for (const [key, value] of Object.entries(table)) {
        if (typeof value === "string") {
          assert.ok(value.trim().length > 0, `${key} is empty`);
        }
      }
    }
  });
});

describe("choosing a locale", () => {
  test("takes the first language we actually have", () => {
    assert.equal(preferred(["ru-RU", "en-US"]), "ru");
    assert.equal(preferred(["en-GB"]), "en");
    // Not a language we have: skip it rather than fall over.
    assert.equal(preferred(["de-DE", "ru"]), "ru");
  });

  test("falls back to English when nothing matches", () => {
    assert.equal(preferred(["de", "fr"]), "en");
    assert.equal(preferred([]), "en");
  });

  test("knows what a locale is", () => {
    assert.equal(isLocale("ru"), true);
    assert.equal(isLocale("de"), false);
    assert.equal(isLocale(null), false);
  });
});
