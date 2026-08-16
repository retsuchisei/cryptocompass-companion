import { For, Show, type JSX } from "solid-js";

import { LOCALES, LOCALE_NAMES, locale, setLocale, t, type Locale } from "../i18n/index.ts";
import { MODES, type Mode } from "../modes.ts";
import { checkForUpdate, installUpdate, updateState } from "../updates.ts";
import { FlagEn, FlagRu } from "./Flags.tsx";
import { Link, type Backend, type Linked } from "./Link.tsx";

/**
 * Preferences, which are not places.
 *
 * Language and seat used to sit in the sidebar beside the screens, which said
 * that choosing "organiser" navigates somewhere. It does not. Anything that
 * changes how the app behaves rather than what it is showing belongs here, and
 * this is where the settings still to come - the session name, the collector,
 * autostart - will land without crowding anything.
 */
function flagFor(item: Locale) {
  return item === "en" ? <FlagEn /> : <FlagRu />;
}

export function Settings(props: {
  mode: Mode;
  onMode: (mode: Mode) => void;
  version: string;
  backend: Backend;
  linked: Linked;
  onLinkChanged: () => void;
}): JSX.Element {
  /**
   * The launch check settles once and then has nothing more to say. Without a
   * way to ask again, an app that checked a minute before a release went out
   * stays a version behind until it is restarted - and the person looking at
   * it cannot tell that from a broken updater.
   */
  const answer = () => {
    const state = updateState();
    switch (state.kind) {
      case "checking": return t().updateChecking;
      case "installing": return t().updateInstalling;
      case "ready": return t().updateReady(state.version);
      case "failed": return t().updateFailed(state.reason);
      default: return t().updateNone;
    }
  };
  return (
    <section class="screen">
      <Link
        backend={props.backend}
        linked={props.linked}
        onChanged={props.onLinkChanged}
      />

      <div class="card">
        <h3>{t().settingsLanguage}</h3>
        <div class="choices">
          <For each={LOCALES}>
            {(item) => (
              <button
                class="choice"
                lang={item}
                aria-pressed={locale() === item}
                classList={{ on: locale() === item }}
                onClick={() => setLocale(item)}
              >
                {flagFor(item)}
                {/* The flag names a country, so the language names itself. */}
                <span>{LOCALE_NAMES[item]}</span>
              </button>
            )}
          </For>
        </div>
      </div>

      <div class="card">
        <h3>{t().settingsUpdates}</h3>
        <dl>
          <dt>{t().settingsVersion}</dt>
          <dd>{props.version}</dd>
        </dl>
        <p class="note">{answer()}</p>
        <div class="choices">
          <button
            class="btn"
            disabled={updateState().kind === "checking" || updateState().kind === "installing"}
            onClick={() => void checkForUpdate()}
          >
            {t().updateCheck}
          </button>
          <Show when={updateState().kind === "ready"}>
            <button class="btn primary" onClick={() => void installUpdate()}>
              {t().updateInstall}
            </button>
          </Show>
        </div>
      </div>

      <div class="card">
        <h3>{t().settingsSeat}</h3>
        <p class="note">{t().settingsSeatHint}</p>
        <div class="choices">
          <For each={MODES}>
            {(each) => (
              <button
                class="choice"
                aria-pressed={props.mode.kind === each.kind}
                classList={{ on: props.mode.kind === each.kind }}
                onClick={() => props.onMode(each)}
              >
                <span>{each.kind === "player" ? t().modePlayer : t().modeOrganiser}</span>
              </button>
            )}
          </For>
        </div>
      </div>
    </section>
  );
}
