/**
 * Asking whether there is a newer build, and taking it.
 *
 * The state is a union rather than a bag of booleans so that "checking" and
 * "an update is waiting" cannot both be true, and so the interface has exactly
 * one thing to render at a time.
 *
 * A failed check is reported, never swallowed. An updater that quietly stops
 * asking is indistinguishable from one that has nothing to offer, and the
 * difference matters: the first means testers are stranded on a build we
 * thought we had replaced.
 */

import { createSignal } from "solid-js";

export type UpdateState =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "none" }
  | { kind: "ready"; version: string }
  | { kind: "installing" }
  | { kind: "failed"; reason: string };

const [updateState, setUpdateState] = createSignal<UpdateState>({ kind: "idle" });

export { updateState };

/** What to say about a thrown value without pretending to know its shape. */
export function reasonOf(error: unknown): string {
  if (error instanceof Error) return error.message;
  return typeof error === "string" ? error : String(error);
}

/** How often to ask again while the window stays open. */
export const ASK_EVERY_MS = 60 * 60 * 1000;

/**
 * Whether asking again is worth doing.
 *
 * Not while a check is in flight, and not once one has been found: the answer
 * would not change and a second check would throw away a version the person is
 * being offered. A previous failure is worth retrying - that is most of why
 * the timer exists.
 */
export function shouldAsk(state: UpdateState): boolean {
  return state.kind !== "checking" && state.kind !== "installing" && state.kind !== "ready";
}

/**
 * Ask once. Returns the state it settled on, so a caller can act without
 * reading the signal back.
 */
export async function checkForUpdate(): Promise<UpdateState> {
  setUpdateState({ kind: "checking" });

  try {
    const { check } = await import("@tauri-apps/plugin-updater");
    const found = await check();
    const next: UpdateState = found
      ? { kind: "ready", version: found.version }
      : { kind: "none" };

    setUpdateState(next);
    return next;
  } catch (error) {
    const next: UpdateState = { kind: "failed", reason: reasonOf(error) };
    setUpdateState(next);
    return next;
  }
}

/** How many times a download is worth retrying before giving up and saying so. */
export const INSTALL_ATTEMPTS = 3;

/** Seconds to wait before attempt n, doubling and capped. */
export function retryAfter(attempt: number): number {
  return Math.min(30, 2 ** attempt);
}

/**
 * Download, install, restart. There is no step between installing and
 * restarting on purpose: a Windows installer that has replaced the running
 * binary leaves the process it replaced in a state nobody should keep using.
 *
 * Retried, because the usual reason a download dies is a connection that comes
 * back a moment later, and asking somebody to press the button again for that
 * is asking them to do the computer's job.
 */
export async function installUpdate(attempts = INSTALL_ATTEMPTS): Promise<void> {
  setUpdateState({ kind: "installing" });

  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      const { check } = await import("@tauri-apps/plugin-updater");
      const found = await check();

      if (!found) {
        setUpdateState({ kind: "none" });
        return;
      }

      await found.downloadAndInstall();

      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
      return;
    } catch (error) {
      const last = attempt === attempts - 1;

      if (last) {
        setUpdateState({ kind: "failed", reason: reasonOf(error) });
        return;
      }

      await new Promise((wake) => setTimeout(wake, retryAfter(attempt) * 1000));
    }
  }
}
