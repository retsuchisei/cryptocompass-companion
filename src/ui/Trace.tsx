import { For, type JSX } from "solid-js";

import { t } from "../i18n/index.ts";

/**
 * Frames arriving, over the last minute.
 *
 * The one thing this window answers from across a room: is the game still
 * talking. Older bars fade, so the right edge is now and how far the colour
 * reaches back is how long it has been talking - the opacity is a reading
 * rather than an effect.
 */
export function Trace(props: {
  bars: number[];
  since: number | null;
}): JSX.Element {
  const quiet = () => props.since === null || props.since > 5;

  return (
    <div class="card stretch">
      <div class="trace" classList={{ quiet: quiet() }} aria-hidden="true">
        <For each={props.bars}>
          {(height, index) => {
            const age = () => index() / Math.max(1, props.bars.length - 1);

            return (
              <i
                style={{
                  height: `${Math.max(2, height * 100)}%`,
                  opacity: quiet() ? 1 : (0.12 + 0.78 * age() ** 1.6).toFixed(3),
                }}
              />
            );
          }}
        </For>
      </div>
      <p class="trace-note">
        {props.since === null ? t().traceQuiet : t().traceLast(props.since.toFixed(1))}
      </p>
    </div>
  );
}
