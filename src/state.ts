/**
 * What the interface knows: whether recording is on, what the Rust side has
 * counted, and whether the person at the keyboard has agreed to any of it.
 *
 * No protocol lives here and no `invoke` either - this module is pure, so the
 * rules about consent can be tested by `node --test` without a window.
 */

/** The shape `status` answers with. Counts and times, never meaning. */
export type Status = {
  version: string;
  recording: boolean;
  /** Unix ms when recording was turned on, or null while it is off. */
  since: number | null;
  sessionId: string | null;
  /** Frames taken from the game since recording was turned on. */
  frames: number;
  lastFrameAt: number | null;
  upstream: UpstreamReport;
};

export type UpstreamReport = {
  state: "off" | "idle" | "connecting" | "live" | "retrying";
  /** Frames handed to the socket - not frames the collector confirmed, which
   * nothing in v1 reports. */
  sent: number;
  pending: number;
  retryInSeconds: number;
  failures: number;
};

export type Recording = { on: false } | { on: true; since: number };

/** Consent carries the version that obtained it, and nothing else. */
export type Consent = { version: string };

/** localStorage's two methods, so the rules can be tested without a browser. */
export type ConsentStore = Pick<Storage, "getItem" | "setItem">;

export const CONSENT_KEY = "consent";

/**
 * Consent belongs to the version it was given for. A bare boolean would
 * outlive the build that explained itself, and "recording is off after every
 * update" would quietly stop being true.
 */
export function consentIsCurrent(
  consent: Consent | null,
  version: string,
): boolean {
  return consent !== null && consent.version === version;
}

export function rememberConsent(
  version: string,
  store: ConsentStore = globalThis.localStorage,
): void {
  const consent: Consent = { version };
  store.setItem(CONSENT_KEY, JSON.stringify(consent));
}

export function consentGiven(
  version: string,
  store: ConsentStore = globalThis.localStorage,
): boolean {
  return consentIsCurrent(readConsent(store), version);
}

/**
 * What is in storage is not ours until it has been checked: an older shape of
 * this app's own, or somebody else's key entirely. Anything that is not a
 * version we can compare is treated as never having been asked.
 */
function readConsent(store: ConsentStore): Consent | null {
  const held = store.getItem(CONSENT_KEY);

  if (held === null) {
    return null;
  }

  try {
    const parsed: unknown = JSON.parse(held);

    if (
      typeof parsed === "object" &&
      parsed !== null &&
      "version" in parsed &&
      typeof parsed.version === "string"
    ) {
      return { version: parsed.version };
    }
  } catch {
    // Not JSON, so not ours.
  }

  return null;
}

/** Recording is on only when the app also says when it started. */
export function recordingOf(status: Status | null): Recording {
  if (status === null || !status.recording || status.since === null) {
    return { on: false };
  }

  return { on: true, since: status.since };
}
