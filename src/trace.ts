/**
 * The frame trace: how much has arrived lately, and how lately.
 *
 * The recorder reports a running count, not a history, so the history is kept
 * here - one sample per poll, differenced into "frames since the last look".
 * Nothing is added to the Rust side for this: a count and a clock are enough
 * to draw arrival, and a backend that also kept a ring buffer would be two
 * places to be wrong about the same thing.
 */

export type Sample = { at: number; frames: number };

/** How many samples the trace remembers. At one poll a second, about a minute. */
export const KEEP = 60;

/**
 * Add a sample, oldest first, and forget what falls off the end.
 *
 * A count that went backwards means a new session rather than a miscount, so
 * the history starts again: carrying the old bars across would draw a spike
 * that never happened.
 */
export function pushSample(history: Sample[], sample: Sample, keep = KEEP): Sample[] {
  const previous = history[history.length - 1];
  const restarted = previous !== undefined && sample.frames < previous.frames;
  const next = restarted ? [sample] : [...history, sample];

  return next.slice(Math.max(0, next.length - keep));
}

/**
 * Bar heights, 0 to 1, oldest first.
 *
 * Scaled to the busiest moment in the window rather than to an absolute rate:
 * the question the trace answers is "is it still arriving", and a fixed scale
 * would answer it with an empty chart on a quiet lobby.
 */
export function bars(history: Sample[]): number[] {
  const deltas: number[] = [];

  for (let i = 1; i < history.length; i += 1) {
    deltas.push(Math.max(0, history[i].frames - history[i - 1].frames));
  }

  const peak = Math.max(...deltas, 1);

  return deltas.map((delta) => delta / peak);
}

/**
 * The bars laid into a fixed number of slots, newest at the right.
 *
 * A trace that stretched five samples across the whole card would say "five
 * enormous frames" when it means "we have been watching for five seconds".
 * Empty slots on the left are the honest picture: the window fills as the
 * history does.
 */
export function slots(heights: number[], count = KEEP): number[] {
  const missing = Math.max(0, count - heights.length);
  return [...new Array(missing).fill(0), ...heights.slice(-count)];
}

/** Seconds since the last frame, or null when none has ever arrived. */
export function sinceLastFrame(lastFrameAt: number | null, now: number): number | null {
  return lastFrameAt === null ? null : Math.max(0, (now - lastFrameAt) / 1000);
}
