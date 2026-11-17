import {diagnosticsHtml,bindDiagnostics} from "./diagnostics";
import {aboutHtml,bindAbout} from "./about";
import {trafficHtml,bindTraffic} from "./traffic";
import {guardHtml,guardBanner,bindGuard,refreshGuard,guardEnabled,guardVerified} from "./guard";
import { applyPreferences, observeLocalization } from "./preferences";
import { networkSettingsHtml, bindNetworkSettings } from "./settings";
import { backupHtml, bindBackup } from "./backup";
import { libraryHtml, bindLibrary } from "./library";
import "./style.css";
import { listen } from "@tauri-apps/api/event";
import { updateMonitor, updateCard, bindUpdates, startUpdater } from "./updater";
import {
  call,
  exitApp,
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
observeLocalization(app);
let state: Snapshot = structuredClone(empty);
let page: "home" | "servers" | "settings" | "activity" | "diagnostics" | "about" = "home";
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
  ? "Проверяем службу VPN…"
  : "Открой настольное приложение";
const probeResults = new Map<string, string>();
let probingId: string | null = null;
let bulkChecking=false;
let bulkCancel=false;

const icons = {
  diagnostics: '<path d="M4 5h16v14H4zM8 9h8M8 13h5"/>',
  about: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6M12 7h.01"/>',
  activity: '<path d="M3 18h18M4 14l5-7 5 4 6-8"/>',
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
  applyPreferences(app,state.settings);
  if (updateMonitor) {
    app.innerHTML = `<main class="update-monitor"><h1>Moth</h1>${updateCard()}</main>`;
    bindUpdates(app);
    applyPreferences(app,state.settings);
    return;
  }
  const linux = state.capabilities.platform === "linux";
  const installationHint = linux
    ? "Установите пакет Moth .deb. Перед обновлением отключите VPN и выйдите из приложения."
    : "Запустите установщик Moth. Для portable-пакета доступен Install-Service.cmd.";
  const selected = state.profiles.find((p) => p.id === state.selected);
  const connected = vpn.phase === "connected" && serviceAvailable && !vpn.error;
  const active =
    vpn.phase === "connected" || vpn.phase === "connecting" || vpn.phase === "disconnecting" || (vpn.phase === "failed" && vpn.ownedByCaller);
  const canCancel = vpnBusy && vpn.phase === "connecting" && vpn.ownedByCaller;
  const status = !serviceAvailable
    ? "Служба недоступна"
    : {
        disconnected: "Не подключено",
        connecting: "Подключаем…",
        connected: vpn.error ? "HTTPS-доступ не подтверждён" : "Подключено",
        disconnecting: "Отключаем…",
        failed: vpn.ownedByCaller ? "Восстановление подключения" : "Соединение прервано",
      }[vpn.phase];
  app.innerHTML = `<div class="shell">
    <aside class="sidebar"><a class="brand" href="#" aria-label="Мотылёк, главная"><span class="brand-mark">${moth}</span><span>мотылёк<small>MOTH DESKTOP</small></span></a>
      <nav aria-label="Разделы">${(
        [
          ["home", "Главная"],
          ["servers", "Серверы"],
          ["activity", "Статистика"],
          ["settings", "Настройки"],
          ["diagnostics", "Диагностика"],
          ["about", "О приложении"],
        ] as const
      )
        .map(
          ([key, title]) =>
            `<button class="nav-item ${page === key ? "active" : ""}" data-page="${key}" ${page === key ? 'aria-current="page"' : ""}>${icon(key)}${title}${key === "servers" ? `<span class="count">${state.profiles.length}</span>` : ""}</button>`,
        )
        .join("")}</nav>
      <div class="sidebar-bottom"><span class="version-dot"></span> Версия для разработки<small>${escapeHtml(state.version)} · ${linux ? "Linux x86_64" : "Windows x64"}</small></div>
    </aside>
    <main><header><div class="breadcrumb">MOTH DESKTOP <span>/</span> ${page === "home" ? "ОБЗОР" : page === "servers" ? "СЕРВЕРЫ" : page === "activity" ? "СТАТИСТИКА" : page === "diagnostics" ? "ДИАГНОСТИКА" : page === "about" ? "О ПРИЛОЖЕНИИ" : "НАСТРОЙКИ"}</div><span class="build-badge">0.3.5</span></header>
    ${!native ? '<div class="preview-note">Просмотр интерфейса в браузере. Профили и подключение доступны только в настольном приложении.</div>' : ""}
    ${error ? `<div class="error" role="alert">${escapeHtml(error)}</div>` : ""}
    ${guardBanner()}<section class="content">
    ${
      page === "home"
        ? `
      <div class="page-heading"><div><p class="eyebrow">VPN</p><h1>Подключение VPN</h1><p class="muted">Выберите сервер и подключитесь к VPN.</p></div><button class="button subtle" data-import>${icon("plus")} Добавить сервер</button></div>
      <div class="overview"><article class="connection-card"><div class="card-top"><span class="status"><i></i> ${status}</span><span class="muted small">СИСТЕМНЫЙ ТУННЕЛЬ</span></div>
        <div class="moth-orbit"><div class="orbit-inner">${moth}</div></div>
        <h2>${!serviceAvailable ? "Служба недоступна" : { disconnected: "VPN отключён", connecting: "Подключение VPN…", connected: vpn.error ? "Ошибка проверки VPN" : "VPN подключён", disconnecting: "Отключение VPN…", failed: vpn.ownedByCaller ? "Восстановление подключения…" : "Ошибка VPN" }[vpn.phase]}</h2><p class="card-description" role="status">${escapeHtml(vpnMessage)}</p>
        <button class="connect" ${!native || !serviceAvailable || (vpnBusy && !canCancel) || (active && !vpn.ownedByCaller) || (!active && !selected) ? "disabled" : ""}>${canCancel ? "Отменить подключение" : vpnBusy ? "Выполняем…" : active ? "Отключить" : "Подключить"}</button><p class="protection-note">${icon("shield")} ${connected ? (guardVerified()?"Туннель проверен · kill switch включён":"Туннель проверен · kill switch не подтверждён") : "VPN сейчас не защищает трафик"}</p>
      </article><div class="right-column"><article class="selected-card"><div class="card-top"><h3>Выбранный сервер</h3><span class="tiny-label">01</span></div>
        ${selected ? `<div class="server-emblem">${icon("servers")}</div><h2 class="selected-name">${escapeHtml(selected.name)}</h2><p class="endpoint">${escapeHtml(selected.host)}:${selected.port}</p><div class="tags"><span>${escapeHtml(selected.protocol)}</span><span>${escapeHtml(selected.security)}</span></div>` : `<div class="server-emblem">${icon("servers")}</div><h2>Нет выбранного сервера</h2><p class="muted">Добавь ссылку от своего провайдера<br>или владельца сервера.</p>`}
        <button class="text-link" data-page="servers">${selected ? "Все серверы" : "Открыть список серверов"} ${icon("arrow")}</button></article>
        <article class="progress-card"><span class="eyebrow">ПОДКЛЮЧЕНИЕ VPN</span><h3>Установка и обновление службы</h3><p>${installationHint}</p><small>${linux ? "Крестик сворачивает окно в трей, а при отсутствии трея — на панель задач." : "Крестик сворачивает Moth в трей."} Для остановки VPN и завершения приложения выберите «Выйти» внизу окна или «Выход» в трее.</small></article></div></div>
      <div class="bottom-note">${icon("shield")}<p><strong>Хранение профилей</strong><span>Профили защищены хранилищем учётной записи. Без аналитики и облачной синхронизации.</span></p></div>
    `
        : page === "servers"
          ? libraryHtml(state, {busy, active, probingId, results:probeResults, icon:icon("servers")})
          : page === "activity" ? trafficHtml()
          : page === "diagnostics" ? `${diagnosticsHtml()}<article class="settings-card"><h2>Служба VPN</h2><div class="setting"><span>Системный VPN</span><strong>${state.capabilities.systemTunnel && serviceAvailable && serviceHealth?.protocol === 2 && serviceHealth.systemTunnel && serviceHealth.engines.length > 0 && serviceHealth.engines.every((e) => e.available) ? "Доступен через службу" : "Не подтверждён"}</strong></div><p class="muted" role="status" aria-label="Состояние службы">${escapeHtml(serviceMessage)}</p>${serviceHealth ? `<div class="setting"><span>Протокол управления</span><strong>${serviceHealth.protocol === 2 ? "Совместим · IPC 2" : "Несовместим"}</strong></div>${serviceHealth.engines.map((e) => `<div class="setting"><span>Ядро службы: ${escapeHtml(e.name)}</span><strong>${escapeHtml(e.version)} · ${e.available ? "Проверено" : "Недоступно"}</strong></div>`).join("")}` : ""}<button class="button subtle service-refresh" ${serviceChecking ? "disabled" : ""}>${serviceChecking ? "Проверяем…" : "Проверить связь со службой"}</button><p class="muted">Для VPN нужна служба Moth с протоколом IPC 2 (0.3.1–0.3.5). ${installationHint}</p></article>`
          : page === "about" ? aboutHtml(state)
          : `<div class="page-heading"><div><p class="eyebrow">ПАРАМЕТРЫ</p><h1>Настройки</h1><p class="muted">Соединение, защита и поведение приложения</p></div></div>
      ${guardHtml()}${networkSettingsHtml(state)}${backupHtml()}`
    }
    </section><footer><span>MOTH · OPEN SOURCE</span><button class="button subtle app-quit" ${native ? "" : "disabled"}>${guardEnabled()?"Выйти, сохранить блокировку":"Выйти"}</button></footer></main></div>
    <dialog id="import-dialog"><form id="import-form"><div class="dialog-heading"><div><p class="eyebrow">ИМПОРТ</p><h2>Добавить серверы</h2></div><button type="button" class="close" aria-label="Закрыть">×</button></div><label for="server-links">Ссылки серверов</label><textarea id="server-links" required rows="6" autocomplete="off" spellcheck="false" placeholder="vless://… или trojan://…"></textarea><p class="dialog-help">По одной ссылке на строку, Base64-список, Clash YAML или поддерживаемый JSON. Для URL провайдера используйте «Добавить подписку».</p><p id="import-error" class="dialog-error" role="alert"></p><div class="dialog-actions"><button type="button" class="button subtle close">Отмена</button><button class="button" type="submit">Импортировать</button></div></form></dialog>
    <dialog id="delete-dialog"><form method="dialog"><h2>Удалить сервер?</h2><p>Он исчезнет из локального списка.</p><div class="dialog-actions"><button class="button subtle" value="cancel">Отмена</button><button class="button" value="delete">Удалить</button></div></form></dialog>`;
  applyPreferences(app,state.settings);
  bindNetworkSettings(app,state,(next)=>{state=next;render();},render);
  bindGuard(app,render);
  bindTraffic(app);
  bindDiagnostics(app);
  bindAbout(app);
  bindBackup(app,(next)=>{state=next;render();});
  bindLibrary(app,state,(next)=>{state=next;render();},render,(e)=>{error=message(e);render();});
  const bulk=app.querySelector<HTMLButtonElement>("[data-probe-all]");
  const cancelBulk=app.querySelector<HTMLButtonElement>("[data-probe-cancel]");
  if(cancelBulk){cancelBulk.hidden=!bulkChecking;cancelBulk.onclick=()=>{bulkCancel=true;};}
  if(bulk) bulk.onclick=async()=>{
    if(bulkChecking||probingId)return;
    const ids=Array.from(app.querySelectorAll<HTMLButtonElement>("[data-probe]")).map(b=>b.dataset.probe!);
    bulkChecking=true;bulkCancel=false;
    try{for(const id of ids){if(bulkCancel)break;probingId=id;render();try{const result=await probe(id);if(result.profileId===id)probeResults.set(id,`HTTPS · ${result.latencyMs} мс`);}catch(e){probeResults.set(id,message(e));}}}
    finally{bulkChecking=false;probingId=null;render();}
  };
  app.querySelector<HTMLAnchorElement>(".brand")!.onclick = (e) => {
    e.preventDefault();
    page = "home";
    render();
  };
  if (page === "settings") {
    app.querySelector("section")!.insertAdjacentHTML("beforeend", updateCard());
    bindUpdates(app);
  }
  const quitButton = app.querySelector<HTMLButtonElement>(".app-quit");
  if (quitButton) quitButton.onclick = async () => {
    try { await exitApp(); }
    catch (e) { error = message(e); render(); }
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
void startUpdater(() => { if (!app.querySelector("dialog[open]")) render(); }).then(() => {
render();
if (updateMonitor) return;
void refreshVpn();
return call("get_snapshot")
  .then((next) => {
    state = next;
    render();
  })
  .catch((e) => {
    error = message(e);
    render();
  });
});

function updateVpnMessage() {
  if(vpn.phase==="failed"&&vpn.ownedByCaller){vpnMessage="Служба восстанавливает подключение. Можно остановить восстановление кнопкой отключения.";return;}
  if(vpn.phase==="connected"&&vpn.error){vpnMessage="Туннель работает, но проверка HTTPS-доступа не прошла. Доступность VPN не подтверждена.";return;}
  vpnMessage = {
    disconnected: "Выбери сервер и подключись.",
    connecting: "Создаём туннель и проверяем HTTPS через него…",
    connected: vpn.ownedByCaller
      ? "Служба подтверждает работу обоих сетевых ядер."
      : "VPN включён другой учётной записью.",
    disconnecting: "Отключаем туннель…",
    failed: "Туннель отключён. Проверь сервер и подключись снова.",
  }[vpn.phase];
}
async function refreshVpn() {
  if (updateMonitor) return;
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
        "Требуется совместимая служба Moth из установочного пакета.",
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
    vpnMessage = message(e) + ". Проверьте установку службы Moth.";
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
let refreshing = false;
setInterval(async () => {
  if (refreshing) return;
  refreshing = true;
  try {
    await refreshVpn();
    if(await refreshGuard()){if(!app.querySelector("dialog[open]"))render();}
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

if(native)void listen("moth-library-changed",async()=>{try{state=await call("get_snapshot");if(!app.querySelector("dialog[open]"))render();}catch(e){error=message(e);}});
