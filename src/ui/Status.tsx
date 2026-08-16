import { Show, type JSX } from "solid-js";

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
      <h2>{recording().on ? "Идёт запись" : "Запись выключена"}</h2>

      <dl>
        <Show when={startedAt()} keyed>
          {(since) => (
            <div>
              <dt>Включена</dt>
              <dd>{atTime(since)}</dd>
            </div>
          )}
        </Show>
        <div>
          <dt>Кадров получено</dt>
          <dd>{props.status.frames}</dd>
        </div>
        <div>
          <dt>Последний кадр</dt>
          <dd>
            {props.status.lastFrameAt === null
              ? "пока нет"
              : atTime(props.status.lastFrameAt)}
          </dd>
        </div>
        <div>
          <dt>Сессия</dt>
          <dd>{props.status.sessionId ?? "не начата"}</dd>
        </div>
        <div>
          <dt>Выгрузка</dt>
          <dd>{upstreamLabel(props.status.upstream)}</dd>
        </div>
      </dl>

      <Show when={recording().on && props.status.frames === 0}>
        <p class="note">
          Кадров пока нет. Игра присылает их только в кастомном матче и только
          если LiveAPI включён в параметрах запуска.
        </p>
      </Show>

      <Show when={props.mode.kind === "organiser"}>
        <p class="note">
          Режим организатора: приложение пока только записывает. Команды лобби -
          создать, настроить, запустить - оно не отправляет.
        </p>
      </Show>

      <Show
        when={recording().on}
        fallback={
          <button class="primary" onClick={() => props.onStart()}>
            Включить запись
          </button>
        }
      >
        <button onClick={() => props.onStop()}>Остановить запись</button>
      </Show>

      <p class="version">версия {props.status.version}</p>
    </section>
  );
}

function upstreamLabel(upstream: UpstreamReport): string {
  const waiting =
    upstream.pending > 0 ? `, в очереди ${upstream.pending}` : "";

  switch (upstream.state) {
    case "off":
      return "не настроена, запись только на этот компьютер";
    case "idle":
      return "ждёт первый кадр";
    case "connecting":
      return `подключается${waiting}`;
    // Sent, not delivered: v1 has no acknowledgement, so a frame handed over
    // just before a drop may or may not have landed and this end cannot tell.
    case "live":
      return `отправлено ${upstream.sent}${waiting}`;
    case "retrying":
      return `нет связи, повтор через ${upstream.retryInSeconds} с (неудач: ${upstream.failures})${waiting}`;
  }
}

function atTime(unixMs: number): string {
  return new Date(unixMs).toLocaleTimeString();
}
