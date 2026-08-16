import { createSignal, For, onCleanup, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

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
  { kind: "record", label: "Запись" },
  { kind: "setup", label: "Настройка игры" },
] as const;

type Screen = (typeof SCREENS)[number]["kind"];

function App() {
  const [status, setStatus] = createSignal<AppStatus | null>(null);
  const [mode, setMode] = createSignal<Mode>(MODES[0]);
  const [screen, setScreen] = createSignal<Screen>("record");
  const [accepted, setAccepted] = createSignal(false);
  const [configuredPath, setConfiguredPath] = createSignal<string | null>(null);
  const [failure, setFailure] = createSignal<string | null>(null);

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
        <h1>CryptoCompass Companion</h1>
        <nav>
          <For each={MODES}>
            {(each) => (
              <button
                classList={{ chosen: mode().kind === each.kind }}
                onClick={() => setMode(each)}
              >
                {each.kind === "player" ? "Игрок" : "Организатор"}
              </button>
            )}
          </For>
        </nav>
      </header>

      <Show when={status()} fallback={<p class="note">Запуск...</p>}>
        {(current) => (
          <Show when={consented()} fallback={<Consent onAccept={() => void accept()} />}>
            <nav class="screens">
              <For each={SCREENS}>
                {(each) => (
                  <button
                    classList={{ chosen: screen() === each.kind }}
                    onClick={() => setScreen(each.kind)}
                  >
                    {each.label}
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

      <Show when={failure()} keyed>
        {(text) => <p class="failure">{text}</p>}
      </Show>
    </main>
  );
}

export default App;
