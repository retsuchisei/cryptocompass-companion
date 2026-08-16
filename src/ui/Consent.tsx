import type { JSX } from "solid-js";

import { t } from "../i18n/index.ts";

/**
 * The one screen that can turn recording on.
 *
 * Every word of it comes from the catalogue. Consent read in a language the
 * reader merely copes with is not consent, which is why this app follows the
 * browser's language rather than greeting everyone in English the way the site
 * does.
 */
export function Consent(props: { onAccept: () => void }): JSX.Element {
  return (
    <section class="screen">
      <h2>{t().consentTitle}</h2>

      <p>{t().consentLobby}</p>
      <p>{t().consentLocalFirst}</p>
      <p>{t().consentOffUntil}</p>
      <p class="note">{t().consentCustomOnly}</p>

      <button class="primary" onClick={() => props.onAccept()}>
        {t().consentAccept}
      </button>
    </section>
  );
}
