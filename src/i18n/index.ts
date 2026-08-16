/**
 * The lookup. `t()` returns the catalogue for the current locale; reading it
 * inside a component is what makes that component follow a language change.
 *
 * The same shape as the zones app's, deliberately: one module-level signal, no
 * context and no provider, because there is one locale for the whole window.
 *
 * Unlike the site, this app defaults to the browser's language when it is one
 * we have. A website is found by strangers and English is the safe greeting; a
 * desktop app is installed on purpose by somebody we handed it to, and the
 * first screen it shows is a consent screen. Consent read in a language you
 * merely cope with is not consent.
 */

import { createSignal } from "solid-js";

import { en, type Strings } from "./en.ts";
import { ru } from "./ru.ts";

export type { Strings } from "./en.ts";

export const LOCALES = ["en", "ru"] as const;

export type Locale = (typeof LOCALES)[number];

export const LOCALE_NAMES: Record<Locale, string> = {
  en: "English",
  ru: "Russian",
};

const TABLES: Record<Locale, Strings> = {
  en,
  // Spread over English, so a key the translation has not reached yet falls
  // back rather than showing a blank or a key name.
  ru: { ...en, ...ru },
};

const KEY = "companion:locale";

export function isLocale(value: unknown): value is Locale {
  return LOCALES.includes(value as Locale);
}

/** The first locale of `navigator.languages` we actually have, or English. */
export function preferred(languages: readonly string[]): Locale {
  for (const tag of languages) {
    const base = tag.toLowerCase().split("-")[0];
    if (isLocale(base)) return base;
  }

  return "en";
}

function stored(): Locale {
  try {
    const saved = localStorage.getItem(KEY);
    if (isLocale(saved)) return saved;
  } catch {
    // A browser refusing storage costs the preference, nothing else.
  }

  return preferred(typeof navigator === "undefined" ? [] : navigator.languages);
}

const [locale, setLocaleSignal] = createSignal<Locale>(stored());

export { locale };

export function setLocale(next: Locale): void {
  setLocaleSignal(next);

  try {
    localStorage.setItem(KEY, next);
  } catch {
    // As above.
  }
}

export function t(): Strings {
  return TABLES[locale()];
}
