import { createSignal, Show, type JSX } from "solid-js";

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
 * Its text is Russian for the same reason the consent screen's is: it is text
 * a user reads, and this app has no message catalogue yet.
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
      return `Игра присылает кадры, последний в ${atTime(proof.at)}. Всё настроено.`;
    }

    return proof.reason === "not-listening"
      ? "Запись выключена, проверять нечего. Включите её на экране записи, потом зайдите в кастомный матч."
      : "Кадров пока нет. Зайдите в кастомный матч - в обычном матчмейкинге и в ранкеде игра ничего не пришлёт.";
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
      <h2>Настройка игры</h2>

      <p>
        Шаг 1. Приложение пропишет игре, куда отправлять поток - на этот
        компьютер, в это приложение. Имя ниже игра запишет в свои логи; подойдёт
        ваш ник.
      </p>

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
          Настроить игру
        </button>
      </div>

      <Show when={props.configuredPath} keyed>
        {(path) => <p class="note">Записано: {path}</p>}
      </Show>

      <p>
        Шаг 2. Один раз добавьте это в параметры запуска игры (EA app: Manage,
        View properties, Advanced launch properties; Steam: свойства, параметры
        запуска) и перезапустите её. Приложение не может сделать это за вас:
        параметры хранит лаунчер и перезаписывает их по-своему.
      </p>

      <div class="row">
        <input class="option" readonly value={LAUNCH_OPTION} ref={field} />
        <button onClick={() => void copy()}>Скопировать</button>
      </div>

      <Show when={copied()}>
        <p class="note">Скопировано.</p>
      </Show>

      <p>
        Шаг 3. Проверка. Мы не можем прочитать параметры запуска, поэтому
        единственное доказательство - пришедший кадр.
      </p>

      <p class="note">{proofLabel()}</p>
    </section>
  );
}

function atTime(unixMs: number): string {
  return new Date(unixMs).toLocaleTimeString();
}
