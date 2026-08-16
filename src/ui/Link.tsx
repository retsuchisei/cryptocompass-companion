import { createSignal, onCleanup, Show, type JSX } from "solid-js";

import { t } from "../i18n/index.ts";

/**
 * Linking this install to an account.
 *
 * The app never becomes a browser. It asks the site for a pairing, shows the
 * six characters, opens the page in whatever browser the person already uses,
 * and waits. The token that comes back never reaches this side of the app: the
 * Rust half fetches it and writes it to disk, and all this screen ever learns
 * is whether it worked.
 *
 * The code is shown here so it can be compared with the one on the page. That
 * comparison is the whole defence, and it runs the other way than it first
 * appears: the danger is not somebody stealing this token, it is somebody being
 * talked into confirming an install that is not theirs.
 */

export type Linked = { linked: boolean; name: string | null };

export type Pairing = { code: string; url: string };

/** Everything the Rust side does for this screen, in one place so a test or a
 * preview can stand in for it. */
export type Backend = {
  linkedAs: () => Promise<Linked>;
  begin: (name: string) => Promise<Pairing>;
  poll: () => Promise<boolean>;
  unlink: () => Promise<void>;
  open: (url: string) => Promise<void>;
};

/** How often to ask whether somebody has confirmed. The pairing dies after five
 * minutes, so this is a few dozen requests at most. */
export const POLL_MS = 2000;

export function Link(props: {
  backend: Backend;
  linked: Linked;
  onChanged: () => void;
}): JSX.Element {
  const [pairing, setPairing] = createSignal<Pairing | null>(null);
  const [name, setName] = createSignal(defaultName());
  const [failed, setFailed] = createSignal<string | null>(null);
  let timer: number | undefined;

  onCleanup(() => window.clearInterval(timer));

  const stopPolling = () => {
    window.clearInterval(timer);
    timer = undefined;
  };

  const start = async () => {
    setFailed(null);

    try {
      const started = await props.backend.begin(name());
      setPairing(started);
      await props.backend.open(started.url);

      timer = window.setInterval(() => void ask(), POLL_MS);
    } catch (error) {
      setFailed(String(error));
    }
  };

  const ask = async () => {
    try {
      if (await props.backend.poll()) {
        stopPolling();
        setPairing(null);
        props.onChanged();
      }
    } catch (error) {
      // "gone" means the pairing expired or was used - the app has to ask for
      // a new one rather than keep polling a code nobody will confirm.
      stopPolling();
      setPairing(null);
      setFailed(String(error));
    }
  };

  const forget = async () => {
    await props.backend.unlink();
    props.onChanged();
  };

  return (
    <div class="card">
      <h3>{t().linkTitle}</h3>

      <Show when={props.linked.linked}>
        <p class="note">{t().linkLinkedAs(props.linked.name ?? "")}</p>
        <div class="choices">
          <button class="btn" onClick={() => void forget()}>
            {t().linkForget}
          </button>
        </div>
      </Show>

      <Show when={!props.linked.linked}>
        <p class="note">{t().linkWhy}</p>

        <Show when={!pairing()}>
          <label class="field">
            <span>{t().linkName}</span>
            <input
              type="text"
              value={name()}
              onInput={(event) => setName(event.currentTarget.value)}
            />
          </label>
          <div class="choices">
            <button class="btn primary" onClick={() => void start()}>
              {t().linkBegin}
            </button>
          </div>
        </Show>

        <Show when={pairing()}>
          {(started) => (
            <>
              <p class="link-code">{started().code}</p>
              <p class="note">{t().linkCompare}</p>
              <div class="choices">
                <button class="btn" onClick={() => void props.backend.open(started().url)}>
                  {t().linkOpenAgain}
                </button>
              </div>
            </>
          )}
        </Show>
      </Show>

      <Show when={failed()}>
        {(reason) => <p class="note bad">{t().linkFailed(reason())}</p>}
      </Show>
    </div>
  );
}

/** The machine's own name where the platform offers one, and something
 * recognisable where it does not. It is only ever a label on a list. */
export function defaultName(): string {
  const guess = window.navigator.platform || "";

  return guess ? `${guess} install` : "this machine";
}
