import type { JSX } from "solid-js";

import { t } from "../i18n/index.ts";

/**
 * The one screen that can turn recording on.
 *
 * It is a screen rather than a dialog because it is not an interruption: it is
 * the first thing this app has to say, and there is nothing behind it worth
 * seeing first. Every word comes from the catalogue - consent read in a
 * language the reader merely copes with is not consent.
 */
export function Consent(props: { onAccept: () => void }): JSX.Element {
  return (
    <section class="screen">
      <div class="card">
        <p>{t().consentLobby}</p>
      </div>
      <div class="card">
        <p>{t().consentLocalFirst}</p>
      </div>
      <div class="card">
        <p>{t().consentOffUntil}</p>
        <p class="note">{t().consentCustomOnly}</p>
      </div>

      <div class="actions">
        <button class="btn primary" onClick={() => props.onAccept()}>
          {t().consentAccept}
        </button>
      </div>
    </section>
  );
}
