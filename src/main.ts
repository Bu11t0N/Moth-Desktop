import "./style.css";
import { call, empty, native, type Snapshot } from "./api";

const app = document.querySelector<HTMLDivElement>("#app")!;
let state: Snapshot = structuredClone(empty);
let page: "home" | "servers" | "settings" = "home";
let busy = false;
let error = "";

const icons = {
  home: '<path d="m3 11 9-8 9 8M5 10v10h5v-6h4v6h5V10"/>',
  servers:
    '<rect x="4" y="3" width="16" height="7" rx="2"/><rect x="4" y="14" width="16" height="7" rx="2"/><path d="M8 6h.01M8 17h.01M12 6h5M12 17h5"/>',
  settings:
    '<path d="M4 7h16M4 17h16"/><circle cx="9" cy="7" r="3"/><circle cx="15" cy="17" r="3"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  arrow: '<path d="m9 5 7 7-7 7"/>',
  shield:
    '<path d="m12 3 8 3v6c0 5-8 9-8 9s-8-4-8-9V6z"/><path d="m8 12 3 3 5-6"/>',
};
const icon = (name: keyof typeof icons) =>
  `<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${icons[name]}</svg>`;
const moth = `<svg viewBox="0 0 160 160" fill="none" aria-hidden="true"><path d="M80 66C63 41 29 23 25 43c-4 20 17 46 43 48-21 0-30 10-26 21 5 13 23 6 38-15 15 21 33 28 38 15 4-11-5-21-26-21 26-2 47-28 43-48-4-20-38-2-55 23Z" stroke="currentColor" stroke-width="2"/><path d="M80 63v39m0-38-9-14m9 14 9-14M32 46l38 38m58-38L90 84M49 111l24-17m38 17L87 94" stroke="currentColor" stroke-width="1.4"/><path d="m80 71-4 13 4 17 4-17-4-13Z" fill="currentColor"/></svg>`;
export const escapeHtml = (text: string) =>
  text.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ]!,
  );

function render() {
  const selected = state.profiles.find((p) => p.id === state.selected);
  app.innerHTML = `<div class="shell">
    <aside class="sidebar"><a class="brand" href="#" aria-label="Мотылёк, главная"><span class="brand-mark">${moth}</span><span>мотылёк<small>MOTH DESKTOP</small></span></a>
      <nav aria-label="Разделы">${(
        [
          ["home", "Главная"],
          ["servers", "Серверы"],
          ["settings", "Настройки"],
        ] as const
      )
        .map(
          ([key, title]) =>
            `<button class="nav-item ${page === key ? "active" : ""}" data-page="${key}" ${page === key ? 'aria-current="page"' : ""}>${icon(key)}${title}${key === "servers" ? `<span class="count">${state.profiles.length}</span>` : ""}</button>`,
        )
        .join("")}</nav>
      <div class="sidebar-bottom"><span class="version-dot"></span> Версия для разработки<small>0.1.0 · Windows first</small></div>
    </aside>
    <main><header><div class="breadcrumb">ЛИЧНОЕ ПРОСТРАНСТВО <span>/</span> ${page === "home" ? "ОБЗОР" : page === "servers" ? "СЕРВЕРЫ" : "НАСТРОЙКИ"}</div><span class="build-badge">PREVIEW 01</span></header>
    ${!native ? '<div class="preview-note">Просмотр интерфейса в браузере. Профили и подключение доступны только в настольном приложении.</div>' : ""}
    ${error ? `<div class="error" role="alert">${escapeHtml(error)}</div>` : ""}
    <section class="content">
    ${
      page === "home"
        ? `
      <div class="page-heading"><div><p class="eyebrow">ТИШЕ. ПРОЩЕ. БЛИЖЕ.</p><h1>Твоя сторона сети.</h1><p class="muted">Знакомый Мотылёк. Теперь на большом экране.</p></div><button class="button subtle" data-import>${icon("plus")} Добавить сервер</button></div>
      <div class="overview"><article class="connection-card"><div class="card-top"><span class="status"><i></i> Не подключено</span><span class="muted small">СИСТЕМНЫЙ ТУННЕЛЬ</span></div>
        <div class="moth-orbit"><div class="orbit-inner">${moth}</div></div>
        <h2>Сначала — надёжная основа</h2><p class="card-description">Импортируй серверы и собери свой список.<br>Подключение VPN появится на следующем этапе.</p>
        <button class="connect" disabled>Подключение в разработке</button><p class="protection-note">${icon("shield")} VPN сейчас не защищает трафик</p>
      </article><div class="right-column"><article class="selected-card"><div class="card-top"><h3>Выбранный сервер</h3><span class="tiny-label">01</span></div>
        ${selected ? `<div class="server-emblem">${icon("servers")}</div><h2 class="selected-name">${escapeHtml(selected.name)}</h2><p class="endpoint">${escapeHtml(selected.host)}:${selected.port}</p><div class="tags"><span>${escapeHtml(selected.protocol)}</span><span>${escapeHtml(selected.security)}</span></div>` : `<div class="server-emblem">${icon("servers")}</div><h2>Пока пусто</h2><p class="muted">Добавь ссылку от своего провайдера<br>или владельца сервера.</p>`}
        <button class="text-link" data-page="servers">${selected ? "Все серверы" : "Открыть список серверов"} ${icon("arrow")}</button></article>
        <article class="progress-card"><span class="eyebrow">ПЕРВЫЙ ПОЛЁТ</span><h3>Растём постепенно</h3><p>Знакомые серверы.<br>Новые возможности на компьютере.</p><div class="milestones"><span class="done"></span><span></span><span></span><span></span></div><small>01 / 04 · Профили и интерфейс</small></article></div></div>
      <div class="bottom-note">${icon("shield")}<p><strong>Твои данные остаются у тебя</strong><span>В Windows профили защищены средствами учётной записи. Без аналитики и облачной синхронизации.</span></p></div>
    `
        : page === "servers"
          ? `
      <div class="page-heading"><div><p class="eyebrow">ТВОИ МАРШРУТЫ</p><h1>Серверы</h1><p class="muted">VLESS и Trojan · ссылки и Base64-списки</p></div><button class="button" data-import>${icon("plus")} Добавить сервер</button></div>
      ${state.profiles.length ? `<div class="server-list">${state.profiles.map((p) => `<article class="server-row ${p.id === state.selected ? "chosen" : ""}"><div class="server-symbol">${icon("servers")}</div><div class="server-info"><h3>${escapeHtml(p.name)}</h3><p>${escapeHtml(p.host)}:${p.port}</p><div class="tags"><span>${escapeHtml(p.protocol)}</span><span>${escapeHtml(p.transport)}</span><span>${escapeHtml(p.security)}</span></div></div><div class="server-actions"><button class="button subtle" data-select="${p.id}" ${busy ? "disabled" : ""}>${p.id === state.selected ? "Выбран" : "Выбрать"}</button><button class="delete" data-delete="${p.id}" ${busy ? "disabled" : ""}>Удалить</button></div></article>`).join("")}</div>` : `<div class="empty-state"><div class="empty-art">${moth}</div><h2>Здесь начнётся твой маршрут</h2><p>Вставь ссылку VLESS или Trojan.<br>Секреты сохраняются только в настольном приложении.</p><button class="button" data-import>${icon("plus")} Добавить первый сервер</button></div>`}
    `
          : `<div class="page-heading"><div><p class="eyebrow">ПОД КРЫЛОМ</p><h1>Настройки</h1><p class="muted">Состояние этой сборки и доступные возможности</p></div></div>
      <article class="settings-card"><h2>Приложение</h2><div class="setting"><span>Версия</span><strong>${escapeHtml(state.version)} · Preview</strong></div><div class="setting"><span>Платформа</span><strong>${escapeHtml(state.capabilities.platform)}</strong></div><div class="setting"><span>Защищённое хранилище</span><strong>${state.capabilities.secureStorage ? "Windows DPAPI" : "Недоступно в этом режиме"}</strong></div><div class="setting"><span>Системный VPN</span><strong>В разработке</strong></div><div class="setting"><span>Kill switch</span><strong>В разработке</strong></div></article>
      <article class="settings-card"><h2>Что дальше</h2><p class="muted">Служба Windows, сетевые ядра, системный туннель и проверка DNS. Затем — восстановление после сбоев, трей и установщик. Настройки подключения появятся вместе с работающими возможностями.</p></article>`
    }
    </section><footer><span>MOTH · OPEN SOURCE</span><span>Спокойное соединение с миром.</span></footer></main></div>
    <dialog id="import-dialog"><form id="import-form"><div class="dialog-heading"><div><p class="eyebrow">НОВЫЙ МАРШРУТ</p><h2>Добавить серверы</h2></div><button type="button" class="close" aria-label="Закрыть">×</button></div><label for="server-links">Ссылки серверов</label><textarea id="server-links" required rows="6" autocomplete="off" spellcheck="false" placeholder="vless://… или trojan://…"></textarea><p class="dialog-help">По одной ссылке на строку или Base64-список. Загрузка HTTPS-подписок будет добавлена позже.</p><p id="import-error" class="dialog-error" role="alert"></p><div class="dialog-actions"><button type="button" class="button subtle close">Отмена</button><button class="button" type="submit">Импортировать</button></div></form></dialog>
    <dialog id="delete-dialog"><form method="dialog"><h2>Удалить сервер?</h2><p>Он исчезнет из локального списка.</p><div class="dialog-actions"><button class="button subtle" value="cancel">Отмена</button><button class="button" value="delete">Удалить</button></div></form></dialog>`;
  app.querySelector<HTMLAnchorElement>(".brand")!.onclick = (e) => {
    e.preventDefault();
    page = "home";
    render();
  };
  app.querySelectorAll<HTMLButtonElement>("[data-page]").forEach(
    (b) =>
      (b.onclick = () => {
        page = b.dataset.page as typeof page;
        error = "";
        render();
      }),
  );
  const dialog = app.querySelector<HTMLDialogElement>("#import-dialog")!;
  app
    .querySelectorAll<HTMLButtonElement>("[data-import]")
    .forEach((b) => (b.onclick = () => dialog.showModal()));
  dialog
    .querySelectorAll<HTMLButtonElement>(".close")
    .forEach((b) => (b.onclick = () => dialog.close()));
  dialog.addEventListener("close", () => {
    dialog.querySelector<HTMLTextAreaElement>("textarea")!.value = "";
    dialog.querySelector("#import-error")!.textContent = "";
  });
  app.querySelector<HTMLFormElement>("#import-form")!.onsubmit = async (e) => {
    e.preventDefault();
    if (busy) return;
    busy = true;
    const submit = dialog.querySelector<HTMLButtonElement>("[type=submit]")!;
    submit.disabled = true;
    try {
      state = await call("import_servers", {
        text: dialog.querySelector<HTMLTextAreaElement>("textarea")!.value,
      });
      dialog.close();
      page = "servers";
      error = "";
    } catch (e) {
      dialog.querySelector("#import-error")!.textContent = message(e);
    } finally {
      busy = false;
      submit.disabled = false;
      if (!dialog.open) render();
    }
  };
  app
    .querySelectorAll<HTMLButtonElement>("[data-select]")
    .forEach(
      (b) => (b.onclick = () => run("select_server", { id: b.dataset.select })),
    );
  app.querySelectorAll<HTMLButtonElement>("[data-delete]").forEach(
    (b) =>
      (b.onclick = () => {
        const d = app.querySelector<HTMLDialogElement>("#delete-dialog")!;
        d.showModal();
        d.onclose = () => {
          if (d.returnValue === "delete")
            void run("remove_server", { id: b.dataset.delete });
        };
      }),
  );
}
const message = (e: unknown) =>
  e instanceof Error
    ? e.message
    : typeof e === "string"
      ? e
      : "Не удалось выполнить действие";
async function run(command: string, args: Record<string, unknown>) {
  if (busy) return;
  busy = true;
  render();
  try {
    state = await call(command, args);
    error = "";
  } catch (e) {
    error = message(e);
  } finally {
    busy = false;
    render();
  }
}
render();
call("get_snapshot")
  .then((next) => {
    state = next;
    render();
  })
  .catch((e) => {
    error = message(e);
    render();
  });
