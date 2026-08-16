import { createSignal, For, Match, onCleanup, onMount, Show, Switch } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

import { LOCALES, LOCALE_NAMES, locale, setLocale, t } from "./i18n/index.ts";
import { checkForUpdate, installUpdate, updateState } from "./updates.ts";
import { MODES, type Mode } from "./modes.ts";
import { consentGiven, rememberConsent, type Status as AppStatus } from "./state.ts";
import { Consent } from "./ui/Consent.tsx";
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
] as const;

type Screen = (typeof SCREENS)[number]["kind"];

function App() {
  const [status, setStatus] = createSignal<AppStatus | null>(null);
  const [mode, setMode] = createSignal<Mode>(MODES[0]);
  const [screen, setScreen] = createSignal<Screen>("record");
  const [accepted, setAccepted] = createSignal(false);
  const [configuredPath, setConfiguredPath] = createSignal<string | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);

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
      setStatus(await invoke<AppStatus>("status"));
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
    <main class="app">
      <header>
        <h1>{t().appTitle}</h1>
        <nav>
          <For each={MODES}>
            {(each) => (
              <button
                classList={{ chosen: mode().kind === each.kind }}
                onClick={() => setMode(each)}
              >
                {each.kind === "player" ? t().modePlayer : t().modeOrganiser}
              </button>
            )}
          </For>
        </nav>

        {/* Two words rather than flags: the app has one strip of chrome and a
            flag says nothing to a reader who does not already know it. */}
        <nav class="locales">
          <For each={LOCALES}>
            {(each) => (
              <button
                classList={{ chosen: locale() === each }}
                aria-label={LOCALE_NAMES[each]}
                onClick={() => setLocale(each)}
              >
                {each.toUpperCase()}
              </button>
            )}
          </For>
        </nav>
      </header>

      <Show when={status()} fallback={<p class="note">{t().starting}</p>}>
        {(current) => (
          <Show when={consented()} fallback={<Consent onAccept={() => void accept()} />}>
            <nav class="screens">
              <For each={SCREENS}>
                {(each) => (
                  <button
                    classList={{ chosen: screen() === each.kind }}
                    onClick={() => setScreen(each.kind)}
                  >
                    {each.kind === "record" ? t().screenRecord : t().screenSetup}
                  </button>
                )}
              </For>
            </nav>

            <Show
              when={screen() === "setup"}
              fallback={
                <Status
                  status={current()}
                  mode={mode()}
                  onStart={() => void start()}
                  onStop={() => void stop()}
                />
              }
            >
              <Setup
                status={current()}
                configuredPath={configuredPath()}
                onConfigure={(sessionName) => void configure(sessionName)}
              />
            </Show>
          </Show>
        )}
      </Show>

      {/* Below the screens rather than above them: an update is worth
          offering, never worth standing between somebody and the thing they
          opened the app to do. */}
      <Show when={updateState().kind !== "idle" && updateState().kind !== "none"}>
        <p class="update">
          <Switch>
            <Match when={updateState().kind === "checking"}>{t().updateChecking}</Match>
            <Match when={updateState().kind === "installing"}>{t().updateInstalling}</Match>
            <Match when={ready()} keyed>
              {(version) => (
                <>
                  {t().updateReady(version)}{" "}
                  <button onClick={() => void installUpdate()}>{t().updateInstall}</button>
                </>
              )}
            </Match>
            <Match when={failedReason()} keyed>
              {(reason) => (
                <>
                  {t().updateFailed(reason)}{" "}
                  <button onClick={() => void checkForUpdate()}>{t().updateRecheck}</button>
                </>
              )}
            </Match>
          </Switch>
        </p>
      </Show>

      <Show when={failure()} keyed>
        {(text) => <p class="failure">{text}</p>}
      </Show>
    </main>
  );
}

export default App;
