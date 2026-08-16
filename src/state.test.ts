import assert from "node:assert/strict";
import { test } from "node:test";

import {
  CONSENT_KEY,
  consentGiven,
  consentIsCurrent,
  launchProof,
  recordingOf,
  rememberConsent,
  type ConsentStore,
  type Status,
} from "./state.ts";

/**
 * localStorage's two methods and nothing else, so what consent is worth can be
 * decided without a browser.
 */
function fakeStore(): ConsentStore {
  const held = new Map<string, string>();

  return {
    getItem: (key) => held.get(key) ?? null,
    setItem: (key, value) => {
      held.set(key, value);
    },
  };
}

/** What the Rust side answers when nothing has happened yet. */
function idleStatus(): Status {
  return {
    version: "0.1.0",
    recording: false,
    since: null,
    sessionId: null,
    frames: 0,
    lastFrameAt: null,
    upstream: {
      state: "off",
      sent: 0,
      pending: 0,
      retryInSeconds: 0,
      failures: 0,
    },
  };
}

test("consent from an older version does not count", () => {
  assert.equal(consentIsCurrent({ version: "0.1.0" }, "0.1.0"), true);
  assert.equal(consentIsCurrent({ version: "0.1.0" }, "0.2.0"), false);
  assert.equal(consentIsCurrent(null, "0.1.0"), false);
});

test("consent survives a restart, and an update asks again", () => {
  const store = fakeStore();
  assert.equal(consentGiven("0.1.0", store), false, "nobody has been asked yet");

  rememberConsent("0.1.0", store);
  assert.equal(consentGiven("0.1.0", store), true);
  assert.equal(
    consentGiven("0.2.0", store),
    false,
    "recording is off after every update, and so is the consent that turned it on",
  );
});

test("what is not a stored consent is not consent", () => {
  const store = fakeStore();

  // Anything can be in local storage: an older shape of this app's own, or
  // somebody's experiment. Only a version we can compare counts.
  for (const junk of ["", "{not json", "true", '{"version":1}', "[]"]) {
    store.setItem(CONSENT_KEY, junk);
    assert.equal(consentGiven("0.1.0", store), false, `accepted ${junk}`);
  }
});

test("recording is on only when the app says when it started", () => {
  assert.deepEqual(recordingOf(null), { on: false }, "no answer yet is not on");
  assert.deepEqual(recordingOf(idleStatus()), { on: false });
  assert.deepEqual(
    recordingOf({ ...idleStatus(), recording: true, since: 1700000000000 }),
    { on: true, since: 1700000000000 },
  );
  assert.deepEqual(
    recordingOf({ ...idleStatus(), recording: true }),
    { on: false },
    "a start with no time behind it is not a start",
  );
});

test("the launch option is proven by a frame, never by having asked", () => {
  const listening: Status = {
    ...idleStatus(),
    recording: true,
    since: 1700000000000,
  };

  // Nothing is listening, so silence says nothing about the launch option.
  assert.deepEqual(launchProof(null), { proven: false, reason: "not-listening" });
  assert.deepEqual(launchProof(idleStatus()), {
    proven: false,
    reason: "not-listening",
  });

  assert.deepEqual(
    launchProof(listening),
    { proven: false, reason: "silent" },
    "recording with nothing arriving is the case the screen exists for",
  );

  assert.deepEqual(
    launchProof({ ...listening, frames: 1, lastFrameAt: 1700000000500 }),
    { proven: true, at: 1700000000500 },
    "the game connected and sent something, which is the only proof there is",
  );

  assert.deepEqual(
    launchProof({ ...listening, frames: 1 }),
    { proven: false, reason: "silent" },
    "a frame with no time behind it is not evidence of anything",
  );
});
