import "./style.css";
import { listen } from "@tauri-apps/api/event";
import {
  call,
  empty,
  native,
  probe,
  serviceStatus,
  tunnel,
  type ServiceHealth,
  type TunnelState,
  type Snapshot,
} from "./api";

const app = document.querySelector<HTMLDivElement>("#app")!;
let state: Snapshot = structuredClone(empty);
let page: "home" | "servers" | "settings" = "home";
let busy = false;
let error = "";
let serviceMessage = "Ещё не проверена";
let serviceChecking = false;
let serviceGeneration = 0;
let serviceHealth: ServiceHealth | null = null;
let vpn: TunnelState = {
  phase: "disconnected",
  ownedByCaller: false,
  error: null,
};
let vpnBusy = false;
let vpnOperation = 0;
let vpnRevision = 0;
let vpnRenderPending = false;
let serviceAvailable = false;
let vpnMessage = native
  ? "Проверяем службу Windows…"
  : "Открой настольное приложение Windows";
const probeResults = new Map<string, string>();
let probingId: string | null = null;

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
  const connected = vpn.phase === "connected" && serviceAvailable;
  const active =
    connected || vpn.phase === "connecting" || vpn.phase === "disconnecting";
  const canCancel = vpnBusy && vpn.phase === "connecting" && vpn.ownedByCaller;
  const status = !serviceAvailable
    ? "Служба недоступна"
    : {
        disconnected: "Не подключено",
        connecting: "Подключаем…",
        connected: "Подключено",
        disconnecting: "Отключаем…",
        failed: "Соединение прервано",
      }[vpn.phase];
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
      <div class="sidebar-bottom"><span class="version-dot"></span> Версия для разработки<small>0.3.3 · Windows x64</small></div>
    </aside>
    <main><header><div class="breadcrumb">MOTH DESKTOP <span>/</span> ${page === "home" ? "ОБЗОР" : page === "servers" ? "СЕРВЕРЫ" : "НАСТРОЙКИ"}</div><span class="build-badge">0.3.3</span></header>
    ${!native ? '<div class="preview-note">Просмотр интерфейса в браузере. Профили и подключение доступны только в настольном приложении.</div>' : ""}
    ${error ? `<div class="error" role="alert">${escapeHtml(error)}</div>` : ""}
    <section class="content">
    ${
      page === "home"
        ? `
      <div class="page-heading"><div><p class="eyebrow">VPN</p><h1>Подключение VPN</h1><p class="muted">Выберите сервер и подключитесь к VPN.</p></div><button class="button subtle" data-import>${icon("plus")} Добавить сервер</button></div>
      <div class="overview"><article class="connection-card"><div class="card-top"><span class="status"><i></i> ${status}</span><span class="muted small">СИСТЕМНЫЙ ТУННЕЛЬ</span></div>
        <div class="moth-orbit"><div class="orbit-inner">${moth}</div></div>
        <h2>${!serviceAvailable ? "Служба недоступна" : { disconnected: "VPN отключён", connecting: "Подключение VPN…", connected: "VPN подключён", disconnecting: "Отключение VPN…", failed: "Ошибка VPN" }[vpn.phase]}</h2><p class="card-description" role="status">${escapeHtml(vpnMessage)}</p>
        <button class="connect" ${!native || !serviceAvailable || (vpnBusy && !canCancel) || (active && !vpn.ownedByCaller) || (!active && !selected) ? "disabled" : ""}>${canCancel ? "Отменить подключение" : vpnBusy ? "Выполняем…" : active ? "Отключить" : "Подключить"}</button><p class="protection-note">${icon("shield")} ${connected ? "Туннель проверен · kill switch пока недоступен" : "VPN сейчас не защищает трафик"}</p>
      </article><div class="right-column"><article class="selected-card"><div class="card-top"><h3>Выбранный сервер</h3><span class="tiny-label">01</span></div>
        ${selected ? `<div class="server-emblem">${icon("servers")}</div><h2 class="selected-name">${escapeHtml(selected.name)}</h2><p class="endpoint">${escapeHtml(selected.host)}:${selected.port}</p><div class="tags"><span>${escapeHtml(selected.protocol)}</span><span>${escapeHtml(selected.security)}</span></div>` : `<div class="server-emblem">${icon("servers")}</div><h2>Нет выбранного сервера</h2><p class="muted">Добавь ссылку от своего провайдера<br>или владельца сервера.</p>`}
        <button class="text-link" data-page="servers">${selected ? "Все серверы" : "Открыть список серверов"} ${icon("arrow")}</button></article>
        <article class="progress-card"><span class="eyebrow">ПОДКЛЮЧЕНИЕ WINDOWS</span><h3>Установка и обновление службы</h3><p>Отключите VPN и запустите Install-Service.cmd из новой папки приложения. Установщик обновит службу и ядра с откатом при ошибке запуска.</p><small>Крестик сворачивает Moth в трей. Для остановки VPN и завершения приложения выбери «Выход» в меню трея.</small></article></div></div>
      <div class="bottom-note">${icon("shield")}<p><strong>Хранение профилей</strong><span>В Windows профили защищены средствами учётной записи. Без аналитики и облачной синхронизации.</span></p></div>
    `
        : page === "servers"
          ? `
      <div class="page-heading"><div><p class="eyebrow">ПРОФИЛИ</p><h1>Серверы</h1><p class="muted">VLESS и Trojan · ссылки и Base64-списки</p></div><button class="button" data-import>${icon("plus")} Добавить сервер</button></div>
      ${state.profiles.length ? `<div class="server-list">${state.profiles.map((p) => `<article class="server-row ${p.id === state.selected ? "chosen" : ""}"><div class="server-symbol">${icon("servers")}</div><div class="server-info"><h3>${escapeHtml(p.name)}</h3><p>${escapeHtml(p.host)}:${p.port}</p><div class="tags"><span>${escapeHtml(p.protocol)}</span><span>${escapeHtml(p.transport)}</span><span>${escapeHtml(p.security)}</span></div></div><div class="server-actions"><button class="button subtle" data-probe="${p.id}" ${busy || probingId ? "disabled" : ""}>${probingId === p.id ? "Проверяем…" : "Проверить сервер"}</button><span class="probe-result" role="status">${escapeHtml(probeResults.get(p.id) || "")}</span><button class="button subtle" data-select="${p.id}" ${busy || active ? "disabled" : ""}>${p.id === state.selected ? "Выбран" : "Выбрать"}</button><button class="delete" data-delete="${p.id}" ${busy || active ? "disabled" : ""}>Удалить</button></div></article>`).join("")}</div>` : `<div class="empty-state"><div class="empty-art">${moth}</div><h2>Нет серверов</h2><p>Вставь ссылку VLESS или Trojan.<br>Секреты сохраняются только в настольном приложении.</p><button class="button" data-import>${icon("plus")} Добавить первый сервер</button></div>`}
    `
          : `<div class="page-heading"><div><p class="eyebrow">ПАРАМЕТРЫ</p><h1>Настройки</h1><p class="muted">Состояние этой сборки и доступные возможности</p></div></div>
      <article class="settings-card"><h2>Приложение</h2><div class="setting"><span>Версия</span><strong>${escapeHtml(state.version)} · Preview</strong></div><div class="setting"><span>Платформа</span><strong>${escapeHtml(state.capabilities.platform)}</strong></div><div class="setting"><span>Защищённое хранилище</span><strong>${state.capabilities.secureStorage ? "Windows DPAPI" : "Недоступно в этом режиме"}</strong></div><div class="setting"><span>Системный VPN</span><strong>${state.capabilities.systemTunnel && serviceAvailable && serviceHealth?.protocol === 2 && serviceHealth.systemTunnel && serviceHealth.engines.length > 0 && serviceHealth.engines.every((e) => e.available) ? "Доступен через службу" : "Не подтверждён"}</strong></div><div class="setting"><span>Kill switch</span><strong>${state.capabilities.killSwitch ? "Доступен" : "Не реализован"}</strong></div></article>
      <article class="settings-card"><h2>Служба Windows</h2><p class="muted" role="status">${escapeHtml(serviceMessage)}</p>${serviceHealth ? `<div class="setting"><span>Протокол управления</span><strong>${serviceHealth.protocol === 2 ? "Совместим · IPC 2" : "Несовместим"}</strong></div>${serviceHealth.engines.map((e) => `<div class="setting"><span>Ядро службы: ${escapeHtml(e.name)}</span><strong>${escapeHtml(e.version)} · ${e.available ? "Проверено" : "Недоступно"}</strong></div>`).join("")}` : ""}<button class="button subtle service-refresh" ${serviceChecking ? "disabled" : ""}>${serviceChecking ? "Проверяем…" : "Проверить связь со службой"}</button><p class="muted">Для VPN нужна служба Moth с протоколом IPC 2 (0.3.1–0.3.3). Запусти Install-Service.cmd рядом с приложением.</p></article><article class="settings-card"><h2>Ядра приложения</h2>${(state.engines || []).map((e) => `<div class="setting"><span>${escapeHtml(e.name)} ${escapeHtml(e.version)}</span><strong>${e.available ? "Хэш проверен при запуске" : escapeHtml(e.message || "Недоступен")}</strong></div>`).join("")}<p class="muted">HTTPS-проверка проходит через выбранный сервер. Её результат — не ICMP ping и не признак включённого системного VPN.</p></article><article class="settings-card"><h2>Ограничения</h2><p class="muted">После смены сети подключись заново. Kill switch и автоматическое восстановление после сна ещё разрабатываются. При сбое ядра туннель отключается, Windows возвращается к обычной сети.</p></article>`
    }
    </section><footer><span>MOTH · OPEN SOURCE</span><span>VPN-клиент для Windows</span></footer></main></div>
    <dialog id="import-dialog"><form id="import-form"><div class="dialog-heading"><div><p class="eyebrow">ИМПОРТ</p><h2>Добавить серверы</h2></div><button type="button" class="close" aria-label="Закрыть">×</button></div><label for="server-links">Ссылки серверов</label><textarea id="server-links" required rows="6" autocomplete="off" spellcheck="false" placeholder="vless://… или trojan://…"></textarea><p class="dialog-help">По одной ссылке на строку или Base64-список. Загрузка HTTPS-подписок будет добавлена позже.</p><p id="import-error" class="dialog-error" role="alert"></p><div class="dialog-actions"><button type="button" class="button subtle close">Отмена</button><button class="button" type="submit">Импортировать</button></div></form></dialog>
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
  app.querySelectorAll<HTMLButtonElement>("[data-probe]").forEach(
    (b) =>
      (b.onclick = async () => {
        const id = b.dataset.probe!;
        if (probingId || busy) return;
        probingId = id;
        render();
        try {
          const result = await probe(id);
          if (result.profileId === id)
            probeResults.set(id, `HTTPS · ${result.latencyMs} мс`);
        } catch (e) {
          probeResults.set(id, message(e));
        } finally {
          probingId = null;
          render();
        }
      }),
  );
  const serviceButton =
    app.querySelector<HTMLButtonElement>(".service-refresh");
  if (serviceButton)
    serviceButton.onclick = async () => {
      if (serviceChecking) return;
      const generation = ++serviceGeneration;
      serviceChecking = true;
      render();
      try {
        const health = await serviceStatus();
        if (generation !== serviceGeneration) return;
        serviceHealth = health;
        serviceMessage =
          health.protocol === 2
            ? `Связь установлена · ${health.version} · протокол ${health.protocol}`
            : `Несовместимый протокол службы · ${health.protocol}`;
      } catch (e) {
        if (generation !== serviceGeneration) return;
        serviceHealth = null;
        serviceMessage = message(e);
      } finally {
        if (generation === serviceGeneration) {
          serviceChecking = false;
          render();
        }
      }
    };
  const connectButton = app.querySelector<HTMLButtonElement>(".connect");
  if (connectButton)
    connectButton.onclick = async () => {
      if (vpnBusy && !canCancel) return;
      const operation = ++vpnOperation;
      ++vpnRevision;
      vpnBusy = true;
      error = "";
      render();
      try {
        const next = await tunnel(
          active ? "disconnect_vpn" : "connect_vpn",
          active ? undefined : state.selected || undefined,
        );
        if (operation !== vpnOperation) return;
        ++vpnRevision;
        vpn = next;
        updateVpnMessage();
      } catch (e) {
        if (operation === vpnOperation) error = message(e);
      } finally {
        if (operation === vpnOperation) {
          ++vpnRevision;
          vpnBusy = false;
          render();
        }
      }
    };
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

function updateVpnMessage() {
  vpnMessage = {
    disconnected: "Выбери сервер и подключись.",
    connecting: "Создаём туннель и проверяем HTTPS через него…",
    connected: vpn.ownedByCaller
      ? "Служба подтверждает работу обоих сетевых ядер."
      : "VPN включён другой учётной записью Windows.",
    disconnecting: "Отключаем туннель…",
    failed: "Туннель отключён. Проверь сервер и подключись снова.",
  }[vpn.phase];
}
async function refreshVpn() {
  if (!native) return;
  const before = JSON.stringify([
    vpn,
    serviceAvailable,
    vpnMessage,
    serviceHealth,
    serviceMessage,
    serviceChecking,
  ]);
  const revision = vpnRevision;
  try {
    const next = await tunnel("tunnel_status");
    if (
      !next ||
      ![
        "disconnected",
        "connecting",
        "connected",
        "disconnecting",
        "failed",
      ].includes(next.phase)
    )
      throw new Error(
        "Требуется совместимая служба Moth. Запусти Install-Service.cmd.",
      );
    if (revision !== vpnRevision) return;
    vpn = next;
    if (!serviceAvailable && !serviceHealth)
      serviceMessage = "Версии службы ещё не проверены";
    serviceAvailable = true;
    updateVpnMessage();
  } catch (e) {
    if (revision !== vpnRevision) return;
    serviceAvailable = false;
    ++serviceGeneration;
    serviceChecking = false;
    serviceHealth = null;
    serviceMessage = message(e);
    vpnMessage = message(e) + ". Запусти Install-Service.cmd из папки Moth.";
  }
  if (
    before !==
      JSON.stringify([
        vpn,
        serviceAvailable,
        vpnMessage,
        serviceHealth,
        serviceMessage,
        serviceChecking,
      ]) ||
    vpnRenderPending
  ) {
    vpnRenderPending = !!app.querySelector("dialog[open]");
    if (!vpnRenderPending) render();
  }
}
void refreshVpn();
let refreshing = false;
setInterval(async () => {
  if (refreshing) return;
  refreshing = true;
  try {
    await refreshVpn();
  } finally {
    refreshing = false;
  }
}, 1500);

if (native) {
  void listen<string>("moth-tray-error", (event) => {
    error = event.payload;
    render();
  }).catch(() => {});
}
