import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { native } from "./api";
type Status = { phase: string; update: { version: string; notes: string; file: string; size: number } | null; downloaded: number; total: number; error: string | null };
let status: Status = { phase: "", update: null, downloaded: 0, total: 0, error: null };
export let updateMonitor = false;
let changed = () => {};
let restarting = false;
const escape = (text: string) => text.replace(/[&<>"']/g, c => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
const messages: Record<string, string> = {
  "": "Проверьте, доступна ли новая версия.", checking: "Проверяем обновления…", current: "Новых подписанных обновлений нет.",
  available: "Доступна новая версия.", downloading: "Скачиваем и проверяем обновление…",
  ready: "Файл скачан, подпись проверена. Можно установить обновление.", authorizing: "Отключаем VPN и запрашиваем разрешение на установку…",
  verifying: "Updater повторно проверяет подпись, версию и файлы…", preparing: "Готовим установку…",
  installing: "Устанавливаем приложение, службу и сетевые компоненты…", rollingBack: "Установка не завершилась. Восстанавливаем предыдущую версию…",
  complete: "Обновление установлено. Открываем Moth…", failed: "Обновление не установлено. Предыдущая версия сохранена; можно повторить попытку.",
  recoveryFailed: "Автоматическое восстановление не подтверждено. Сохраните VPN отключённым и повторно установите полный пакет Moth.", error: "Не удалось завершить обновление.",
};
export function updateCard(): string {
  const working = ["checking", "downloading", "authorizing", "verifying", "preparing", "installing", "rollingBack"].includes(status.phase);
  return `<article class="settings-card update-card"><h2>${updateMonitor ? "Установка обновления" : "Обновления"}</h2>
    ${status.update ? `<p>Версия ${escape(status.update.version)}</p>${status.update.notes ? `<p class="update-notes">${escape(status.update.notes)}</p>` : ""}` : ""}
    <p role="status">${escape(messages[status.phase] || "Проверяем состояние установки…")}</p>
    ${status.phase === "downloading" && status.total > 0 ? `<progress max="${status.total}" value="${status.downloaded}" aria-label="Скачивание обновления"></progress><p>${(status.downloaded / 1048576).toFixed(1)} / ${(status.total / 1048576).toFixed(1)} МБ</p>` : ""}
    ${status.error ? `<p role="alert">${escape(status.error)}</p>` : ""}
    ${updateMonitor ? (["complete", "failed"].includes(status.phase) ? '<button class="button" data-update="restart_updated_app">Открыть Moth</button>' : "") : `
      <button class="button subtle" data-update="check_updates" ${!native || working ? "disabled" : ""}>Проверить обновления</button>
      ${status.update && !working && status.phase !== "ready" ? '<button class="button" data-update="download_update">Скачать обновление</button>' : ""}
      ${status.phase === "ready" ? '<p class="muted">VPN будет отключён. Для установки потребуется разрешение администратора. Профили сохранятся.</p><button class="button" data-update="install_update">Установить обновление</button>' : ""}
    `}</article>`;
}
export function bindUpdates(root: HTMLElement) {
  root.querySelectorAll<HTMLButtonElement>("[data-update]").forEach(button => {
    button.onclick = async () => {
      button.disabled = true;
      try { const next = await invoke<Status | undefined>(button.dataset.update!); if (next) status = next; }
      catch (error) { status.error = typeof error === "string" ? error : "Не удалось выполнить обновление"; status.phase = "error"; }
      changed();
    };
  });
}
export async function startUpdater(render: () => void) {
  changed = render;
  if (!native) return;
  updateMonitor = await invoke<boolean>("update_monitor_mode").catch(() => false) === true;
  if (!updateMonitor) { await listen<Status>("moth-update-progress", event => { status = event.payload; changed(); }).catch(() => {}); return; }
  const poll = async () => {
    try {
      status = await invoke<Status>("update_status"); changed();
      if (status.phase === "complete" && !restarting) {
        restarting = true;
        setTimeout(() => { void invoke("restart_updated_app").catch(() => { status.error = "Нажмите «Открыть Moth», чтобы запустить приложение."; changed(); }); }, 800);
      }
    } catch { status.error = "Не удалось прочитать состояние установки. Установка может продолжаться; не запускайте её повторно."; changed(); }
  };
  await poll();
  let pending = false;
  setInterval(() => { if (pending) return; pending = true; void poll().finally(() => { pending = false; }); }, 750);
}
