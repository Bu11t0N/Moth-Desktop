import { invoke } from "@tauri-apps/api/core";
import { native, type Snapshot, type TunnelState } from "./api";
import { currentLanguage } from "./preferences";

type Group = { name: string; kind: string; members: string[]; providers?: string[]; selected?: string };
type Preview = { token: string; summary: {
  canApply: boolean;
  nodeOnlyCount: number;
  nodes: {name: string; kind: string}[];
  groups: Group[];
  providers: {name: string; kind: string; remote: boolean}[];
  rules: {kind: string; selector?: string; target: string}[];
  dnsMode: string | null;
  dnsFields: string[];
  issues: {field: string; reason: string}[];
  managedFields: string[];
} };
const t = (ru: string, en: string) => currentLanguage() === "en" ? en : ru;
const escape = (value: string) => value.replace(/[&<>"']/g, char => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
})[char]!);
const previews = new WeakMap<HTMLDialogElement, Preview>();

export function looksLikeClash(text: string): boolean {
  return /(?:^|\n)\s*["']?(?:proxies|proxy-groups|proxy-providers|rule-providers|dns|rules)["']?\s*:/m.test(text)
    || (/^\s*\{/.test(text) && /"(?:proxies|proxy-groups|proxy-providers|rule-providers|dns|rules)"\s*:/.test(text));
}
export function clashDialog(): string {
  return `<dialog id="clash-preview" aria-labelledby="clash-preview-title">
    <div class="dialog-heading"><h2 id="clash-preview-title">${t("Предпросмотр Clash", "Clash import preview")}</h2>
    <button class="close" type="button" data-clash-cancel aria-label="${t("Закрыть", "Close")}">×</button></div>
    <div class="clash-preview-list" data-clash-summary></div>
    <label for="clash-policy-name">${t("Название профиля", "Profile name")}</label>
    <input id="clash-policy-name" maxlength="80" autocomplete="off">
    <p role="alert" data-clash-error></p>
    <div class="dialog-actions">
       <button class="button subtle" data-clash-import="nodes">${t("Только совместимые узлы", "Compatible nodes only")}</button>
       <button class="button primary" data-clash-import="full">${t("Сохранить профиль целиком", "Save full profile")}</button>
    </div></dialog>`;
}
function list(title: string, lines: string[]): string {
  return `<details><summary>${title} (${lines.length})</summary><ol>${lines.map(line => `<li>${escape(line)}</li>`).join("")}</ol></details>`;
}
export async function openClashPreview(
  root: HTMLElement, input: {text?: string; subscription?: string}, previous?: HTMLDialogElement,
): Promise<void> {
  const preview = await invoke<Preview | null>("preview_clash", input);
  if (!preview) throw new Error(t("Документ Clash не найден", "No Clash document found"));
  const dialog = root.querySelector<HTMLDialogElement>("#clash-preview")!;
  const s = preview.summary;
  dialog.querySelector<HTMLElement>("[data-clash-summary]")!.innerHTML = `
    <p>${s.canApply
       ? t("Профиль можно сохранить после проверки в Mihomo.", "The full profile can be saved after validation with Mihomo.")
       : t("Политика несовместима. Отдельный импорт узлов не сохранит её маршруты, группы и DNS.", "Full policy is incompatible. Importing nodes separately discards policy routes, groups and DNS.")}</p>
    ${s.issues.length ? `<ul class="error">${s.issues.map(i => `<li><b>${escape(i.field)}</b> · ${escape(i.reason)}</li>`).join("")}</ul>` : ""}
    ${list(t("Узлы", "Nodes"), s.nodes.map(n => `${n.name} · ${n.kind}`))}
    ${list(t("Группы", "Groups"), s.groups.map(g => `${g.name} · ${g.kind} · ${[...g.members, ...(g.providers ?? [])].join(", ")}`))}
    ${list(t("Источники", "Providers"), s.providers.map(p => `${p.name} · ${p.kind} · ${p.remote ? "HTTPS" : "inline"}`))}
    ${list(t("Порядок правил", "Rule order"), s.rules.map(r => `${r.kind}${r.selector ? " · " + r.selector : ""} → ${r.target}`))}
    <p>DNS · ${escape(s.dnsMode ?? t("Настройки Moth", "Moth settings"))}${s.dnsFields.length ? ` · ${escape(s.dnsFields.join(", "))}` : ""}</p>
    ${s.managedFields.length ? `<p>${t("Moth задаст параметры слушателей", "Moth controls listener settings")} · ${escape(s.managedFields.join(", "))}</p>` : ""}`;
  dialog.querySelector<HTMLInputElement>("input")!.value = "Clash";
  dialog.querySelector<HTMLElement>("[data-clash-error]")!.textContent = "";
  dialog.querySelector<HTMLButtonElement>('[data-clash-import="full"]')!.disabled = !s.canApply;
  dialog.querySelector<HTMLButtonElement>('[data-clash-import="nodes"]')!.disabled = !s.nodeOnlyCount;
  previews.set(dialog, preview);
  previous?.close();
  dialog.showModal();
}

export function clashPanel(state: Snapshot, active: boolean, owned: boolean): string {
  const settings = state.settings;
  const mode = state.proxyPreferences?.mode === "proxy" ? "HTTP/SOCKS" : "TUN";
  const saved = state.clashPolicies ?? [];
  return `<article class="settings-card"><h2>${t("Профили Clash", "Clash profiles")}</h2>
    <p class="muted">${t("Импортируйте YAML или откройте предпросмотр в сохранённой подписке.", "Import YAML or open the preview for an existing subscription.")}</p>
    <form data-clash-file-form><label>${t("Файл YAML", "YAML file")}
      <input data-clash-file type="file" accept=".yaml,.yml,.json" ${native ? "" : "disabled"}></label>
      <button class="button subtle" ${native ? "" : "disabled"}>${t("Предпросмотр", "Preview")}</button></form>
    <p role="alert" data-clash-panel-error></p><p aria-live="polite" data-clash-panel-status></p>
    ${saved.length ? `<p class="muted">${escape(mode)} · MTU ${settings?.mtu ?? 1500} · IPv6 ${settings?.ipv6 === false ? t("выкл.", "off") : t("вкл.", "on")}.
      ${t("TUN обрабатывает DNS и IPv6, затем split по приложениям и правила Moth по порядку. Остальное обрабатывает Clash. Для Clash требуется выключенный kill switch.", "TUN applies DNS and IPv6 handling, app splitting and ordered Moth rules. Remaining traffic follows the Clash policy. Clash requires the kill switch to be disabled.")}</p>` : ""}
    ${saved.map(p => `<div class="setting"><div><strong>${escape(p.name)}</strong>
      <p class="muted">${p.refreshFailed ? t("Ошибка обновления. Рабочая копия сохранена.", "Refresh failed. The last working copy is retained.") : t("Готов к подключению", "Ready to connect")}</p></div>
      <div class="library-buttons">
        <button class="button subtle" data-clash-connect="${p.id}" ${active ? "disabled" : ""}>${t("Подключить", "Connect")}</button>
        <button class="button subtle" data-clash-refresh="${p.id}">${t("Обновить", "Refresh")}</button>
        <button class="button subtle" data-clash-remove="${p.id}" ${active ? "disabled" : ""}>${t("Удалить", "Remove")}</button>
      </div></div>`).join("")}
    ${active && owned && saved.length ? `<button class="button subtle" data-clash-groups>${t("Показать активные группы", "Show active groups")}</button><div data-clash-group-list></div>` : ""}
  </article>`;
}

export function bindClash(root: HTMLElement, update: (state: Snapshot) => void, connected: (state: TunnelState) => void) {
  const dialog = root.querySelector<HTMLDialogElement>("#clash-preview")!;
  const issue = dialog.querySelector<HTMLElement>("[data-clash-error]")!;
  const panelIssue = root.querySelector<HTMLElement>("[data-clash-panel-error]");
  const panelStatus = root.querySelector<HTMLElement>("[data-clash-panel-status]");
  let pending = false;
  const run = async (action: () => Promise<void>, target: HTMLElement | null = panelIssue) => {
    if (pending) return;
    pending = true;
    if (target) target.textContent = "";
    try { await action(); }
    catch (error) {
      if (target) target.textContent = typeof error === "string" ? error : error instanceof Error ? error.message : t("Операция не завершилась", "The action failed");
    }
    finally { pending = false; }
  };
  function buttons(selector: string, action: (button: HTMLButtonElement) => Promise<void>, error = panelIssue) {
    root.querySelectorAll<HTMLButtonElement>(selector).forEach(button => {
      button.onclick = () => void run(() => action(button), error);
    });
  }
  dialog.querySelector<HTMLButtonElement>("[data-clash-cancel]")!.onclick = () => dialog.close();
  buttons("[data-clash-import]", async button => {
    const preview = previews.get(dialog);
    if (!preview) return;
    const nodesOnly = button.dataset.clashImport === "nodes";
    const name = dialog.querySelector<HTMLInputElement>("input")!.value.trim();
    if (!nodesOnly && !name) throw new Error(t("Укажите название", "Enter a name"));
    const state = await invoke<Snapshot>("apply_clash", { token: preview.token, name, nodesOnly });
    dialog.close();
    update(state);
  }, issue);
  const form = root.querySelector<HTMLFormElement>("[data-clash-file-form]");
  if (form) form.onsubmit = event => {
    event.preventDefault();
    void run(async () => {
      const file = form.querySelector<HTMLInputElement>("[data-clash-file]")!.files?.[0];
      if (!file || file.size > 192 * 1024) throw new Error(t("Выберите файл до 192 КиБ", "Choose a file up to 192 KiB"));
      await openClashPreview(root, { text: await file.text() });
    });
  };
  buttons("[data-source-clash]", button => openClashPreview(root, {subscription: button.dataset.sourceClash!}));
  buttons("[data-clash-connect]", async button => {
    if (panelStatus) panelStatus.textContent = t("Проверяем подключение…", "Verifying connection…");
    try { connected(await invoke<TunnelState>("connect_clash", {id: button.dataset.clashConnect})); }
    finally { if (panelStatus) panelStatus.textContent = ""; }
  });
  buttons("[data-clash-refresh]", async button => {
    update(await invoke<Snapshot>("refresh_clash", {id: button.dataset.clashRefresh}));
    const status = root.querySelector<HTMLElement>("[data-clash-panel-status]");
    if (status) status.textContent = t("Данные обновлены. Новая конфигурация применяется при следующем подключении.", "Refreshed. The new configuration applies at the next connection.");
  });
  buttons("[data-clash-remove]", async button => {
    update(await invoke<Snapshot>("remove_clash", {id: button.dataset.clashRemove}));
  });
  const groupList = root.querySelector<HTMLElement>("[data-clash-group-list]");
  const renderGroups = (groups: Group[]) => {
    if (!groupList) return;
    groupList.innerHTML = groups.length ? groups.map((g, i) => `<div class="setting"><span>${escape(g.name)} · ${escape(g.kind)}</span>
      ${g.kind === "select" ? `<select aria-label="${escape(g.name)}" data-group-index="${i}">${g.members.map(m => `<option ${m === g.selected ? "selected" : ""}>${escape(m)}</option>`).join("")}</select>` : `<strong>${escape(g.selected ?? "")}</strong>`}</div>`).join("") : t("Нет активных групп", "No active groups");
    groupList.querySelectorAll<HTMLSelectElement>("[data-group-index]").forEach(select => {
      select.onchange = () => void run(async () => {
        const group = groups[Number(select.dataset.groupIndex)];
        try { renderGroups(await invoke<Group[]>("clash_groups", {group: group.name, member: select.value})); }
        catch (error) { renderGroups(groups); throw error; }
      });
    });
  };
  buttons("[data-clash-groups]", async () => renderGroups(await invoke<Group[]>("clash_groups")));
}
