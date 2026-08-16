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
  updateChecking: "Checking for updates...",
  updateReady: (version: string) => `Version ${version} is available.`,
  updateInstall: "Install and restart",
  updateInstalling: "Downloading and installing...",
  updateFailed: (reason: string) => `Could not check for updates: ${reason}`,
  updateRecheck: "Check again",
  windowMinimise: "Minimise",
  windowMaximise: "Maximise",
  windowRestore: "Restore",
  windowClose: "Close",
  settingsVersion: "Version",
  settingsUpdates: "Updates",
  updateNone: "This is the newest build.",
  updateCheck: "Check for updates",
  dismiss: "Dismiss",
  navSettings: "Settings",
  settingsLanguage: "Language",
  settingsSeat: "Seat",
  settingsSeatHint:
    "An organiser records from the observer's seat. Either way the app records the same match; the seat only decides what it will be able to do once lobby commands exist.",
  traceQuiet: "Nothing arriving yet",
  traceLast: (seconds: string) => `Frames arriving - last one ${seconds} s ago`,
  statusIdleNote:
    "Recording is off: nothing is written and nothing is sent. The game only sends frames in a custom match.",
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
  statusFrames: "Recorded",
  statusLastFrame: "Last frame",
  statusNoFrameYet: "none yet",
  statusSession: "Session",
  statusNoSession: "not started",
  statusUpstream: "Sent to us",
  statusNoFramesHint:
    "No frames yet. The game sends them only in a custom match, and only if LiveAPI is enabled in its launch options.",
  statusOrganiserNote:
    "Organiser mode: the app only records so far. It does not send lobby commands - create, configure, start.",
  statusStart: "Turn recording on",
  statusStop: "Stop recording",
  statusVersion: (version: string) => `version ${version}`,

  upstreamOff: "nowhere - this build keeps everything on your computer",
  upstreamIdle: "nothing yet",
  upstreamConnecting: (queued: number) =>
    queued > 0 ? `connecting, ${queued} waiting` : "connecting",
  // Sent, not delivered: v1 has no acknowledgement, so a frame handed over
  // just before a drop may or may not have landed and this end cannot tell.
  upstreamLive: (sent: number, queued: number) =>
    queued > 0 ? `${sent}, ${queued} waiting` : `${sent}`,
  upstreamRetrying: (seconds: number, queued: number) =>
    queued > 0
      ? `no connection, trying again in ${seconds}s, ${queued} waiting`
      : `no connection, trying again in ${seconds}s`,

  setupTitle: "Setting the game up",
  setupStepOne: "Step 1",
  setupStepTwo: "Step 2",
  setupStepThree: "Step 3",
  setupStep1:
    "The app tells the game where to send its stream - to this computer, to this app. The name below is what the game writes in its own logs; your nickname will do.",
  setupConfigure: "Configure the game",
  setupWritten: (path: string) => `Written: ${path}`,
  setupStep2:
    "Add this to the game's launch options once (EA app: Manage, View properties, Advanced launch properties; Steam: properties, launch options) and restart it. The app cannot do this for you: the launcher keeps those options and rewrites them its own way.",
  setupCopy: "Copy",
  setupCopied: "Copied.",
  setupStep3:
    "Proof. We cannot read the launch options, so the only proof is a frame that arrived.",
  setupProven: (at: string) =>
    `The game is sending frames, the last at ${at}. Everything is set up.`,
  setupNotListening:
    "Recording is off, so there is nothing to check. Turn it on from the recording screen, then join a custom match.",
  setupNoFrames:
    "No frames yet. Join a custom match - in ordinary matchmaking and in ranked the game sends nothing.",
};

export type Strings = typeof en;
