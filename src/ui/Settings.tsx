import { For, type JSX } from "solid-js";

import { LOCALES, LOCALE_NAMES, locale, setLocale, t, type Locale } from "../i18n/index.ts";
import { MODES, type Mode } from "../modes.ts";
import { FlagEn, FlagRu } from "./Flags.tsx";

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
}): JSX.Element {
  return (
    <section class="screen">
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
