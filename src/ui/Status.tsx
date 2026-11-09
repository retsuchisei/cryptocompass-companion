import { Show, type JSX } from "solid-js";

import { t } from "../i18n/index.ts";
import {deliveryLabel,recordingProblem} from "../delivery.ts";
import type { Mode } from "../modes.ts";
import { Trace } from "./Trace.tsx";
import {
  recordingOf,
  type Status as AppStatus,
} from "../state.ts";

/**
 * What the app has counted, and the switch that stops it.
 *
 * Counts, times and connection state - never what a frame meant. Reading
 * meaning out of the stream is the server's job, and this screen would go
 * stale the moment the schema moved.
 */
export function Status(props: {
  status: AppStatus;
  mode: Mode;
  bars: number[];
  since: number | null;
  onStart: () => void;
  onStop: () => void;
}): JSX.Element {
  const recording = () => recordingOf(props.status);

  return (
    <section class="screen">
      <Trace bars={props.bars} since={props.since} />

      <div class="card">
        <dl>
          <dt>{t().statusFrames}</dt>
          <dd>{props.status.frames}</dd>
          <dt>{t().statusUpstream}</dt>
          <dd>{deliveryLabel(props.status.upstream,t())}</dd>
        </dl>
      </div>

      <Show when={props.status.error}>
        <p class="failure">{recordingProblem(props.status.error,t()) ?? t().errorCapture}</p>
      </Show>

      <Show when={recording().on && props.status.frames === 0}>
        <p class="note">{t().statusNoFramesHint}</p>
      </Show>

      <Show when={!recording().on}>
        <p class="note">{t().statusIdleNote}</p>
      </Show>

      <Show when={props.mode.kind === "organiser"}>
        <p class="note">{t().statusOrganiserNote}</p>
      </Show>

      <div class="actions">
        <Show
          when={recording().on}
          fallback={
            <button class="btn primary" onClick={() => props.onStart()}>
              {t().statusStart}
            </button>
          }
        >
          <button class="btn" onClick={() => props.onStop()}>
            {t().statusStop}
          </button>
        </Show>
      </div>
    </section>
  );
}
