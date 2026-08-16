import { createSignal, onCleanup, onMount, Show, type JSX } from "solid-js";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { t } from "../i18n/index.ts";

/**
 * The window's own title bar, because the system one belongs to a different
 * palette and this window is dark by design.
 *
 * `data-tauri-drag-region` hands the drag back to the operating system rather
 * than moving the window from JavaScript: the OS is what knows about snapping
 * to an edge and about a second monitor with a different scale factor.
 * Double-click still maximises, which is a habit nobody should have to unlearn.
 */
export function Titlebar(props: {
  /** What the window is doing, shown in the middle where it stays readable. */
  state: string;
  recording: boolean;
  /** The version waiting to be installed, or null when there is none. */
  updateReady: string | null;
  updateInstalling: boolean;
  onInstall: () => void;
}): JSX.Element {
  const [maximised, setMaximised] = createSignal(false);
  const window = getCurrentWindow();

  onMount(() => {
    void window.isMaximized().then(setMaximised);
    const stop = window.onResized(() => void window.isMaximized().then(setMaximised));
    onCleanup(() => void stop.then((off) => off()));
  });

  return (
    <header class="titlebar" data-tauri-drag-region>
      <img class="titlebar-mark" src="/logo.svg" alt="" width="16" height="16" data-tauri-drag-region />
      <span class="titlebar-name" data-tauri-drag-region>
        {t().appTitle}
      </span>

      {/* The middle carries what the window is doing. It is the one line worth
          reading from a glance at a taskbar preview, and it is here rather than
          on a screen because it is true whichever screen is open. */}
      <span class="titlebar-state" data-tauri-drag-region>
        <span class="dot" classList={{ live: props.recording }} />
        {props.state}
      </span>

      {/* An offer, not an interruption: it appears only when there is one, and
          it sits where a thing you may want lives rather than across the work. */}
      <Show when={props.updateReady} keyed>
        {(version) => (
          <button
            class="titlebar-update"
            title={t().updateReady(version)}
            aria-label={t().updateReady(version)}
            onClick={() => props.onInstall()}
          >
            <svg viewBox="0 0 12 12" width="13" height="13" aria-hidden="true">
              <path d="M6 1v7M3 5.5L6 8.5l3-3M2 10.5h8" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </button>
        )}
      </Show>

      <Show when={props.updateInstalling}>
        <span class="titlebar-installing">{t().updateInstalling}</span>
      </Show>

      <div class="titlebar-controls">
        <button aria-label={t().windowMinimise} onClick={() => void window.minimize()}>
          <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
            <path d="M0 5h10" stroke="currentColor" stroke-width="1" />
          </svg>
        </button>
        <button
          aria-label={maximised() ? t().windowRestore : t().windowMaximise}
          onClick={() => void window.toggleMaximize()}
        >
          <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
            {maximised() ? (
              <>
                <path d="M2.5 0.5h7v7h-7z" fill="none" stroke="currentColor" stroke-width="1" />
                <path d="M0.5 2.5h7v7h-7z" fill="var(--ground)" stroke="currentColor" stroke-width="1" />
              </>
            ) : (
              <path d="M0.5 0.5h9v9h-9z" fill="none" stroke="currentColor" stroke-width="1" />
            )}
          </svg>
        </button>
        <button class="close" aria-label={t().windowClose} onClick={() => void window.close()}>
          <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
            <path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1" />
          </svg>
        </button>
      </div>
    </header>
  );
}
