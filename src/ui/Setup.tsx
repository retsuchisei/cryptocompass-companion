import { createSignal, Show, type JSX } from "solid-js";

import { t } from "../i18n/index.ts";
import { launchProof, type Status as AppStatus } from "../state.ts";

/**
 * The two halves of setting the game up: the file the app writes, and the one
 * launch option only the person at the keyboard can set.
 *
 * The option is not written here and cannot be. It lives inside the Steam or
 * EA client's own store, which rewrites the file underneath anything that
 * edits it, so this screen asks for one paste and then proves it took by
 * waiting for a frame - never by saying it worked.
 *
 */
const LAUNCH_OPTION = "+cl_liveapi_enabled 1";

/** What the game calls the recording. Any name will do; a nickname is easiest
 * to recognise later in a log somebody else is reading. */
const DEFAULT_SESSION_NAME = "companion";

export function Setup(props: {
  status: AppStatus;
  configuredPath: string | null;
  onConfigure: (sessionName: string) => void;
}): JSX.Element {
  const [sessionName, setSessionName] = createSignal(DEFAULT_SESSION_NAME);
  const [copied, setCopied] = createSignal(false);
  let field: HTMLInputElement | undefined;

  const proofLabel = () => {
    const proof = launchProof(props.status);

    if (proof.proven) {
      return t().setupProven(atTime(proof.at));
    }

    return proof.reason === "not-listening"
      ? t().setupNotListening
      : t().setupNoFrames;
  };

  async function copy() {
    // Selected first: a webview may refuse the clipboard, and then Ctrl+C on
    // a selection is the fallback rather than nothing at all.
    field?.select();

    try {
      await navigator.clipboard.writeText(LAUNCH_OPTION);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  return (
    <section class="screen">
      <h2>{t().setupTitle}</h2>

      <p>{t().setupStep1}</p>

      <div class="row">
        <input
          class="name"
          value={sessionName()}
          onInput={(event) => setSessionName(event.currentTarget.value)}
        />
        <button
          class="primary"
          disabled={sessionName().trim() === ""}
          onClick={() => props.onConfigure(sessionName().trim())}
        >
          {t().setupConfigure}
        </button>
      </div>

      <Show when={props.configuredPath} keyed>
        {(path) => <p class="note">{t().setupWritten(path)}</p>}
      </Show>

      <p>{t().setupStep2}</p>

      <div class="row">
        <input class="option" readonly value={LAUNCH_OPTION} ref={field} />
        <button onClick={() => void copy()}>{t().setupCopy}</button>
      </div>

      <Show when={copied()}>
        <p class="note">{t().setupCopied}</p>
      </Show>

      <p>{t().setupStep3}</p>

      <p class="note">{proofLabel()}</p>
    </section>
  );
}

function atTime(unixMs: number): string {
  return new Date(unixMs).toLocaleTimeString();
}
