import type { JSX } from "solid-js";

/**
 * The one screen that can turn recording on.
 *
 * Its text is Russian because it is the only text here a user reads, and it is
 * written where it is displayed because this app has no message catalogue yet.
 * Everything else in the repository is English.
 */
export function Consent(props: { onAccept: () => void }): JSX.Element {
  return (
    <section class="screen">
      <h2>Прежде чем включить запись</h2>

      <p>
        Поток LiveAPI описывает весь лобби, а не только вас: события матча и
        действия всех игроков, которые в нём находятся. Включая запись, вы
        отправляете данные и о них тоже.
      </p>
      <p>
        Каждый кадр сначала пишется на этот компьютер, а потом уходит на наш
        сервер. Из таких записей строятся данные о кольцах.
      </p>
      <p>
        Запись выключена, пока вы не нажмёте кнопку ниже. После каждого
        обновления приложение спросит снова.
      </p>
      <p class="note">
        LiveAPI работает только в кастомных матчах: в обычном матчмейкинге и в
        ранкеде игра не пришлёт ничего.
      </p>

      <button class="primary" onClick={() => props.onAccept()}>
        Понимаю, включить запись
      </button>
    </section>
  );
}
