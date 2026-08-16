import { createMemo, createSignal, For, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

import { t } from "./i18n/index.ts";
import { checkForUpdate, installUpdate, updateState } from "./updates.ts";
import { MODES, type Mode } from "./modes.ts";
import { consentGiven, rememberConsent, type Status as AppStatus } from "./state.ts";
import { bars, pushSample, sinceLastFrame, slots, type Sample } from "./trace.ts";
import { Consent } from "./ui/Consent.tsx";
import { Settings } from "./ui/Settings.tsx";
import { Setup } from "./ui/Setup.tsx";
import { Status } from "./ui/Status.tsx";
import "./App.css";

/**
 * The shell: which mode, which screen, and the only place that asks the Rust
 * side to start.
 *
 * Counters are polled rather than pushed. A frame arrives a few times a second
 * and nobody watches a counter that closely, so an event channel would be a
 * moving part bought for nothing.
 */
const POLL_MS = 1000;

/** The two screens consent opens: what the app has counted, and what the game
 * still needs before it sends anything. */
const SCREENS = [
  { kind: "record" },
  { kind: "setup" },
  { kind: "settings" },
] as const;

type Screen = (typeof SCREENS)[number]["kind"];

function App() {
  const [status, setStatus] = createSignal<AppStatus | null>(null);
  const [mode, setMode] = createSignal<Mode>(MODES[0]);
  const [screen, setScreen] = createSignal<Screen>("record");
  const [accepted, setAccepted] = createSignal(false);
  const [configuredPath, setConfiguredPath] = createSignal<string | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);
  const [history, setHistory] = createSignal<Sample[]>([]);
  const [now, setNow] = createSignal(Date.now());

  /** One name per screen, so the sidebar and the header cannot disagree. */
  const label = (kind: Screen) =>
    kind === "setup" ? t().screenSetup : kind === "settings" ? t().navSettings : t().screenRecord;

  const recording = () => status()?.recording === true;

  /** Elapsed since recording started, mm:ss or hh:mm:ss, never a bare count. */
  const elapsed = createMemo(() => {
    // The recorder already knows when it started; keeping a second answer
    // here would be two clocks to disagree.
    const from = status()?.since ?? null;
    if (from === null) return null;
    const total = Math.max(0, Math.floor((now() - from) / 1000));
    const parts = [Math.floor(total / 3600), Math.floor((total % 3600) / 60), total % 60];
    return parts
      .slice(parts[0] === 0 ? 1 : 0)
      .map((part, index) => (index === 0 && parts[0] === 0 ? String(part) : String(part).padStart(2, "0")))
      .join(":");
  });

  // Narrowings rather than casts inside the markup: `keyed` wants a value, and
  // a union member's field is only reachable once the kind is known.
  const ready = () => {
    const state = updateState();
    return state.kind === "ready" ? state.version : null;
  };

  const failedReason = () => {
    const state = updateState();
    return state.kind === "failed" ? state.reason : null;
  };

  // Asked once, on launch, and never again on a timer: this app is opened for
  // a match and closed afterwards, so a poll would spend its life asking a
  // question nobody is waiting on. The button in the failure state is the
  // second ask.
  onMount(() => void checkForUpdate());

  async function refresh() {
    try {
      const next = await invoke<AppStatus>("status");
      setStatus(next);
      setNow(Date.now());
      setHistory((was) => pushSample(was, { at: Date.now(), frames: next.frames }));
      setFailure(null);
    } catch (error) {
      setFailure(String(error));
    }
  }

  void refresh();
  const timer = setInterval(() => void refresh(), POLL_MS);
  onCleanup(() => clearInterval(timer));

  /** Consent belongs to this version, and local storage is not reactive. */
  const consented = () => {
    const current = status();
    return current !== null && (accepted() || consentGiven(current.version));
  };

  async function start() {
    const current = status();

    // The guard, in one place: nothing may start recording without a consent
    // given for the build that is running.
    if (current === null || !consented()) {
      return;
    }

    try {
      await invoke("start_recording");
    } catch (error) {
      setFailure(String(error));
    }

    await refresh();
  }

  async function stop() {
    try {
      await invoke("stop_recording");
    } catch (error) {
      setFailure(String(error));
    }

    await refresh();
  }

  /** The only thing that touches the game's own files, and it writes one. */
  async function configure(sessionName: string) {
    try {
      setConfiguredPath(await invoke<string>("configure_game", { sessionName }));
      setFailure(null);
    } catch (error) {
      setConfiguredPath(null);
      setFailure(String(error));
    }
  }

  async function accept() {
    const current = status();

    if (current === null) {
      return;
    }

    rememberConsent(current.version);
    setAccepted(true);
    await start();
  }

  return (
    <div class="app">
      <aside class="side">
        <div class="brand">
          <img src="/logo.svg" alt="" width="26" height="26" />
          <b>CryptoCompass</b>
        </div>

        {/* One group needs no heading: a category label earns its place when
            there is a second category to tell it apart from. */}
        <nav class="group">
          <For each={SCREENS}>
            {(each) => (
              <button
                class="item"
                classList={{ on: screen() === each.kind }}
                onClick={() => setScreen(each.kind)}
              >
                {label(each.kind)}
              </button>
            )}
          </For>
        </nav>

        <div class="spacer" />

        {/* Status, not settings: the dot and the session are what somebody
            glances at, and everything they can change lives on one screen. */}
        <div class="me">
          <span class="dot" classList={{ live: recording() }} />
          <span class="sub">{status()?.sessionId ?? t().statusNoSession}</span>
        </div>
      </aside>

      <section class="main">
        <div class="bar">
          <h2>{label(screen())}</h2>
          <Show when={recording() && elapsed()} keyed>
            {(text) => <span class="clock">{text}</span>}
          </Show>
        </div>

        <div class="body">
          <Show when={status()} fallback={<p class="note">{t().starting}</p>}>
            {(current) => (
              <Show
                when={consented()}
                fallback={<Consent onAccept={() => void accept()} />}
              >
                <Switch>
                  <Match when={screen() === "setup"}>
                    <Setup
                      status={current()}
                      configuredPath={configuredPath()}
                      onConfigure={(sessionName) => void configure(sessionName)}
                    />
                  </Match>
                  <Match when={screen() === "settings"}>
                    <Settings mode={mode()} onMode={setMode} />
                  </Match>
                  <Match when={true}>
                    <Status
                      status={current()}
                      mode={mode()}
                      bars={slots(bars(history()))}
                      since={sinceLastFrame(current().lastFrameAt, now())}
                      onStart={() => void start()}
                      onStop={() => void stop()}
                    />
                  </Match>
                </Switch>
              </Show>
            )}
          </Show>
        </div>

        {/* Below the screens, never between somebody and what they opened the
            app to do. */}
        <Show when={updateState().kind !== "idle" && updateState().kind !== "none"}>
          <p class="update">
            <Switch>
              <Match when={updateState().kind === "checking"}>{t().updateChecking}</Match>
              <Match when={updateState().kind === "installing"}>{t().updateInstalling}</Match>
              <Match when={ready()} keyed>
                {(version) => (
                  <>
                    {t().updateReady(version)}{" "}
                    <button class="btn small" onClick={() => void installUpdate()}>
                      {t().updateInstall}
                    </button>
                  </>
                )}
              </Match>
              <Match when={failedReason()} keyed>
                {(reason) => (
                  <>
                    {t().updateFailed(reason)}{" "}
                    <button class="btn small" onClick={() => void checkForUpdate()}>
                      {t().updateRecheck}
                    </button>
                  </>
                )}
              </Match>
            </Switch>
          </p>
        </Show>

        <Show when={failure()} keyed>
          {(text) => <p class="failure">{text}</p>}
        </Show>
      </section>
    </div>
  );
}

export default App;
