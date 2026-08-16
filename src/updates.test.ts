import { test, describe } from "node:test";
import assert from "node:assert/strict";

import { reasonOf, retryAfter, shouldAsk } from "./updates.ts";

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

describe("asking again on a timer", () => {
  test("asks after a failure, which is most of why the timer exists", () => {
    assert.equal(shouldAsk({ kind: "failed", reason: "no connection" }), true);
  });

  test("asks when the last answer was that there is nothing", () => {
    assert.equal(shouldAsk({ kind: "none" }), true);
    assert.equal(shouldAsk({ kind: "idle" }), true);
  });

  test("does not ask over a check already in flight", () => {
    assert.equal(shouldAsk({ kind: "checking" }), false);
    assert.equal(shouldAsk({ kind: "installing" }), false);
  });

  test("does not throw away an update already being offered", () => {
    // Re-checking here would replace a version the person is looking at with
    // the same answer, and the button under their cursor would flicker.
    assert.equal(shouldAsk({ kind: "ready", version: "0.1.5" }), false);
  });
});

describe("retrying a download", () => {
  test("waits longer each time, because the usual cause passes", () => {
    assert.equal(retryAfter(0), 1);
    assert.equal(retryAfter(1), 2);
    assert.equal(retryAfter(2), 4);
  });

  test("stops growing, so a long outage is not a long wait", () => {
    assert.equal(retryAfter(10), 30);
  });
});
