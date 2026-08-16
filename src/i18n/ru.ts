/**
 * The Russian catalogue.
 *
 * Partial by design: what is missing here falls back to English rather than
 * showing a blank or a key name. Comments stay English, as everywhere else.
 */

import type { Strings } from "./en.ts";

export const ru: Partial<Strings> = {
  lang: "ru",
  appTitle: "CryptoCompass Companion",
  starting: "Запуск...",

  modePlayer: "Игрок",
  modeOrganiser: "Организатор",

  screenRecord: "Запись",
  screenSetup: "Настройка игры",
  updateChecking: "Проверяю обновления...",
  updateReady: (version: string) => `Доступна версия ${version}.`,
  updateInstall: "Установить и перезапустить",
  updateInstalling: "Скачиваю и устанавливаю...",
  updateFailed: (reason: string) => `Не удалось проверить обновления: ${reason}`,
  updateRecheck: "Проверить ещё раз",
  windowMinimise: "Свернуть",
  windowMaximise: "Развернуть",
  windowRestore: "Восстановить",
  windowClose: "Закрыть",
  settingsVersion: "Версия",
  settingsUpdates: "Обновления",
  updateNone: "Это самая свежая сборка.",
  updateCheck: "Проверить обновления",
  dismiss: "Скрыть",
  errorPortInUse:
    "Порт 7777 уже занят, и игре некуда подключаться. Обычно это вторая копия этого приложения - закрой её и попробуй снова.",
  navSettings: "Настройки",
  settingsLanguage: "Язык",
  settingsSeat: "Место",
  settingsSeatHint:
    "Организатор пишет с места обсервера. Матч в обоих случаях записывается один и тот же; место решает только, что приложение сможет делать, когда появятся команды лобби.",
  traceQuiet: "Пока ничего не приходит",
  traceLast: (seconds: string) => `Кадры идут - последний ${seconds} с назад`,
  statusIdleNote:
    "Запись выключена: ничего не пишется и никуда не уходит. Игра присылает кадры только в кастомном матче.",
  consentTitle: "Прежде чем включить запись",
  consentLobby:
    "Поток LiveAPI описывает весь лобби, а не только вас: события матча и действия всех игроков, которые в нём находятся. Включая запись, вы отправляете данные и о них тоже.",
  consentLocalFirst:
    "Каждый кадр сначала пишется на этот компьютер, а потом уходит на наш сервер. Из таких записей строятся данные о кольцах.",
  consentOffUntil:
    "Запись выключена, пока вы не нажмёте кнопку ниже. После каждого обновления приложение спросит снова.",
  consentCustomOnly:
    "LiveAPI работает только в кастомных матчах: в обычном матчмейкинге и в ранкеде игра не пришлёт ничего.",
  consentAccept: "Понимаю, включить запись",

  statusRecording: "Идёт запись",
  statusStopped: "Запись выключена",
  statusSince: "Включена",
  statusFrames: "Записано",
  statusLastFrame: "Последний кадр",
  statusNoFrameYet: "пока нет",
  statusSession: "Сессия",
  statusNoSession: "не начата",
  statusUpstream: "Ушло к нам",
  statusNoFramesHint:
    "Кадров пока нет. Игра присылает их только в кастомном матче и только если LiveAPI включён в параметрах запуска.",
  statusOrganiserNote:
    "Режим организатора: приложение пока только записывает. Команды лобби - создать, настроить, запустить - оно не отправляет.",
  statusStart: "Включить запись",
  statusStop: "Остановить запись",
  statusVersion: (version: string) => `версия ${version}`,

  upstreamOff: "никуда - эта сборка держит всё на твоём компьютере",
  upstreamIdle: "пока ничего",
  upstreamConnecting: (queued: number) =>
    queued > 0 ? `подключается, ждёт ${queued}` : "подключается",
  upstreamLive: (sent: number, queued: number) =>
    queued > 0 ? `${sent}, ждёт ${queued}` : `${sent}`,
  upstreamRetrying: (seconds: number, queued: number) =>
    queued > 0
      ? `нет связи, повтор через ${seconds} с, ждёт ${queued}`
      : `нет связи, повтор через ${seconds} с`,

  setupTitle: "Настройка игры",
  setupStepOne: "Шаг 1",
  setupStepTwo: "Шаг 2",
  setupStepThree: "Шаг 3",
  setupStep1:
    "Приложение пропишет игре, куда отправлять поток - на этот компьютер, в это приложение. Имя ниже игра запишет в свои логи; подойдёт ваш ник.",
  setupConfigure: "Настроить игру",
  setupWritten: (path: string) => `Записано: ${path}`,
  setupStep2:
    "Один раз добавьте это в параметры запуска игры (EA app: Manage, View properties, Advanced launch properties; Steam: свойства, параметры запуска) и перезапустите её. Приложение не может сделать это за вас: параметры хранит лаунчер и перезаписывает их по-своему.",
  setupCopy: "Скопировать",
  setupCopied: "Скопировано.",
  setupStep3:
    "Проверка. Мы не можем прочитать параметры запуска, поэтому единственное доказательство - пришедший кадр.",
  setupProven: (at: string) =>
    `Игра присылает кадры, последний в ${at}. Всё настроено.`,
  setupNotListening:
    "Запись выключена, проверять нечего. Включите её на экране записи, потом зайдите в кастомный матч.",
  setupNoFrames:
    "Кадров пока нет. Зайдите в кастомный матч - в обычном матчмейкинге и в ранкеде игра ничего не пришлёт.",
};
