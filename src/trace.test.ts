import { test, describe } from "node:test";
import assert from "node:assert/strict";

import { bars, pushSample, sinceLastFrame, slots } from "./trace.ts";

describe("keeping the trace", () => {
  test("remembers samples oldest first and forgets the rest", () => {
    let history: ReturnType<typeof pushSample> = [];
    for (let i = 0; i < 5; i += 1) {
      history = pushSample(history, { at: i, frames: i }, 3);
    }
    assert.deepEqual(history.map((s) => s.frames), [2, 3, 4]);
  });

  test("starts again when the count goes backwards", () => {
    // A restart is a new session, not a miscount. Carrying the old bars over
    // would draw a spike that never happened.
    const history = pushSample(
      [{ at: 1, frames: 900 }, { at: 2, frames: 950 }],
      { at: 3, frames: 4 },
    );
    assert.deepEqual(history, [{ at: 3, frames: 4 }]);
  });
});

describe("drawing the trace", () => {
  test("is one bar shorter than the samples, because a bar is a difference", () => {
    const history = [
      { at: 1, frames: 0 },
      { at: 2, frames: 5 },
      { at: 3, frames: 5 },
    ];
    assert.equal(bars(history).length, 2);
  });

  test("scales to the busiest moment on screen", () => {
    const history = [
      { at: 1, frames: 0 },
      { at: 2, frames: 2 },
      { at: 3, frames: 12 },
    ];
    // Ten is the peak, so two is a fifth of it - not a fifth of some rate we
    // decided in advance.
    assert.deepEqual(bars(history), [0.2, 1]);
  });

  test("draws nothing rather than dividing by zero when nothing arrived", () => {
    const history = [
      { at: 1, frames: 7 },
      { at: 2, frames: 7 },
    ];
    assert.deepEqual(bars(history), [0]);
  });

  test("has no bars at all from a single sample", () => {
    assert.deepEqual(bars([{ at: 1, frames: 3 }]), []);
  });
});

describe("how long since a frame", () => {
  test("is null when none has ever arrived", () => {
    assert.equal(sinceLastFrame(null, 1000), null);
  });

  test("is in seconds, and never negative when the clocks disagree", () => {
    assert.equal(sinceLastFrame(1000, 3500), 2.5);
    assert.equal(sinceLastFrame(5000, 4000), 0);
  });
});

describe("laying the trace into the window", () => {
  test("fills from the right, so a short history is short rather than stretched", () => {
    assert.deepEqual(slots([1, 0.5], 5), [0, 0, 0, 1, 0.5]);
  });

  test("keeps only what fits, newest first to go nowhere", () => {
    assert.deepEqual(slots([1, 2, 3, 4], 2), [3, 4]);
  });

  test("is all empty when nothing has arrived", () => {
    assert.deepEqual(slots([], 3), [0, 0, 0]);
  });
});
