import { test, describe } from "node:test";
import assert from "node:assert/strict";

import { reasonOf } from "./updates.ts";

/**
 * The updater's failures arrive as whatever the plugin threw, and the screen
 * has to say something rather than render "[object Object]". Only the shaping
 * is tested here - the plugin itself needs a window and a network.
 */
describe("reporting an update failure", () => {
  test("uses an Error's message", () => {
    assert.equal(reasonOf(new Error("no connection")), "no connection");
  });

  test("passes a thrown string through", () => {
    assert.equal(reasonOf("signature mismatch"), "signature mismatch");
  });

  test("says something for anything else rather than nothing", () => {
    assert.equal(reasonOf({ code: 500 }), "[object Object]");
    assert.equal(reasonOf(undefined), "undefined");
  });
});
