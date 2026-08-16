import { Show, type JSX } from "solid-js";

import { t } from "../i18n/index.ts";
import type { Mode } from "../modes.ts";
import {
  recordingOf,
  type Status as AppStatus,
  type UpstreamReport,
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
  onStart: () => void;
  onStop: () => void;
}): JSX.Element {
  const recording = () => recordingOf(props.status);

  const startedAt = () => {
    const state = recording();
    return state.on ? state.since : null;
  };

  return (
    <section class="screen">
      <h2>{recording().on ? t().statusRecording : t().statusStopped}</h2>

      <dl>
        <Show when={startedAt()} keyed>
          {(since) => (
            <div>
              <dt>{t().statusSince}</dt>
              <dd>{atTime(since)}</dd>
            </div>
          )}
        </Show>
        <div>
          <dt>{t().statusFrames}</dt>
          <dd>{props.status.frames}</dd>
        </div>
        <div>
          <dt>{t().statusLastFrame}</dt>
          <dd>
            {props.status.lastFrameAt === null
              ? t().statusNoFrameYet
              : atTime(props.status.lastFrameAt)}
          </dd>
        </div>
        <div>
          <dt>{t().statusSession}</dt>
          <dd>{props.status.sessionId ?? t().statusNoSession}</dd>
        </div>
        <div>
          <dt>{t().statusUpstream}</dt>
          <dd>{upstreamLabel(props.status.upstream)}</dd>
        </div>
      </dl>

      <Show when={recording().on && props.status.frames === 0}>
        <p class="note">{t().statusNoFramesHint}</p>
      </Show>

      <Show when={props.mode.kind === "organiser"}>
        <p class="note">{t().statusOrganiserNote}</p>
      </Show>

      <Show
        when={recording().on}
        fallback={
          <button class="primary" onClick={() => props.onStart()}>
            {t().statusStart}
          </button>
        }
      >
        <button onClick={() => props.onStop()}>{t().statusStop}</button>
      </Show>

      <p class="version">{t().statusVersion(props.status.version)}</p>
    </section>
  );
}

function upstreamLabel(upstream: UpstreamReport): string {
  switch (upstream.state) {
    case "off":
      return t().upstreamOff;
    case "idle":
      return t().upstreamIdle;
    case "connecting":
      return t().upstreamConnecting(upstream.pending);
    case "live":
      return t().upstreamLive(upstream.sent, upstream.pending);
    case "retrying":
      return t().upstreamRetrying(
        upstream.retryInSeconds,
        upstream.failures,
        upstream.pending,
      );
  }
}

function atTime(unixMs: number): string {
  return new Date(unixMs).toLocaleTimeString();
}
