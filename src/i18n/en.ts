/**
 * The English catalogue, and the shape every other locale is checked against.
 *
 * English is the default, so this file has no fallback behind it: a key
 * missing here is a compile error at the call site, while a key missing from a
 * translation quietly falls back to this one.
 *
 * Entries are plain values where the text is fixed and functions where it is
 * not. There is no message format and no library - interpolation is a template
 * literal, which is the whole of what this interface needs. A function also
 * lets each language own its own word order, which matters for the upload
 * states: "sent 12, 3 queued" and its Russian counterpart do not compose from
 * the same pieces.
 */

export const en = {
  lang: "en",
  appTitle: "CryptoCompass Companion",
  starting: "Starting...",

  modePlayer: "Player",
  modeOrganiser: "Organiser",

  screenRecord: "Recording",
  screenSetup: "Setting the game up",
  consentTitle: "Before you turn recording on",
  consentLobby:
    "The LiveAPI stream describes the whole lobby, not only you: match events and what every player in it does. Turning recording on sends their data too.",
  consentLocalFirst:
    "Every frame is written to this computer first and goes to our server afterwards. Ring data is built out of those recordings.",
  consentOffUntil:
    "Recording stays off until you press the button below. After every update the app asks again.",
  consentCustomOnly:
    "LiveAPI only works in custom matches: in ordinary matchmaking and in ranked the game sends nothing.",
  consentAccept: "I understand, turn recording on",

  statusRecording: "Recording",
  statusStopped: "Recording is off",
  statusSince: "Started",
  statusFrames: "Frames received",
  statusLastFrame: "Last frame",
  statusNoFrameYet: "none yet",
  statusSession: "Session",
  statusNoSession: "not started",
  statusUpstream: "Upload",
  statusNoFramesHint:
    "No frames yet. The game sends them only in a custom match, and only if LiveAPI is enabled in its launch options.",
  statusOrganiserNote:
    "Organiser mode: the app only records so far. It does not send lobby commands - create, configure, start.",
  statusStart: "Turn recording on",
  statusStop: "Stop recording",
  statusVersion: (version: string) => `version ${version}`,

  upstreamOff: "not configured, recording to this computer only",
  upstreamIdle: "waiting for the first frame",
  upstreamConnecting: (queued: number) =>
    queued > 0 ? `connecting, ${queued} queued` : "connecting",
  // Sent, not delivered: v1 has no acknowledgement, so a frame handed over
  // just before a drop may or may not have landed and this end cannot tell.
  upstreamLive: (sent: number, queued: number) =>
    queued > 0 ? `${sent} sent, ${queued} queued` : `${sent} sent`,
  upstreamRetrying: (seconds: number, failures: number, queued: number) =>
    queued > 0
      ? `no connection, retrying in ${seconds}s (failures: ${failures}), ${queued} queued`
      : `no connection, retrying in ${seconds}s (failures: ${failures})`,

  setupTitle: "Setting the game up",
  setupStep1:
    "Step 1. The app tells the game where to send its stream - to this computer, to this app. The name below is what the game writes in its own logs; your nickname will do.",
  setupConfigure: "Configure the game",
  setupWritten: (path: string) => `Written: ${path}`,
  setupStep2:
    "Step 2. Add this to the game's launch options once (EA app: Manage, View properties, Advanced launch properties; Steam: properties, launch options) and restart it. The app cannot do this for you: the launcher keeps those options and rewrites them its own way.",
  setupCopy: "Copy",
  setupCopied: "Copied.",
  setupStep3:
    "Step 3. Proof. We cannot read the launch options, so the only proof is a frame that arrived.",
  setupProven: (at: string) =>
    `The game is sending frames, the last at ${at}. Everything is set up.`,
  setupNotListening:
    "Recording is off, so there is nothing to check. Turn it on from the recording screen, then join a custom match.",
  setupNoFrames:
    "No frames yet. Join a custom match - in ordinary matchmaking and in ranked the game sends nothing.",
};

export type Strings = typeof en;
