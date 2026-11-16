import {
  call,
  type Snapshot,
  type ConnectionSettings,
  type RoutingRule,
} from "./api";
const e = (s: string) =>
  s.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ]!,
  );
export const defaultSettings: ConnectionSettings = {
  mtu: 1500,
  ipv6: true,
  dns: "https://1.1.1.1/dns-query",
  bootstrapDns: "https://1.1.1.1/dns-query",
  rules: [],
  splitMode: "bypass",
  splitApps: [],
  healthEnabled: false,
  healthIntervalSeconds: 300,
  healthUrl: "https://cp.cloudflare.com/generate_204",
  autoReconnect: false,
  failover: false,
  autoConnect: false,
  theme: "system",
  language: "ru",
};
let draft: ConnectionSettings | null = null;
const options = (choices: string[][], value: string) =>
  choices
    .map(
      ([key, label]) =>
        `<option value="${key}" ${key === value ? "selected" : ""}>${label}</option>`,
    )
    .join("");
export function networkSettingsHtml(state: Snapshot): string {
  const s = draft || state.settings || defaultSettings;
  const check = (name: keyof ConnectionSettings, label: string) =>
    `<label><input type="checkbox" name="${name}" ${s[name] ? "checked" : ""}> ${label}</label>`;
  return `<article class="settings-card"><h2>Сеть и поведение подключения</h2><form id="network-settings" class="network-form">
  <p class="muted">Сетевые настройки применяются при следующем подключении. Общий DNS остаётся внутри VPN, в том числе при разделении приложений.</p>
  <div class="network-grid"><label>MTU<input name="mtu" type="number" min="1280" max="9000" required value="${s.mtu}"></label><label>DNS (IP или HTTPS URL)<input name="dns" required value="${e(s.dns)}"></label><label>Bootstrap DNS (числовой IP в адресе)<input name="bootstrapDns" required value="${e(s.bootstrapDns)}"></label></div>
  ${check("ipv6", "IPv6 через VPN; при выключении IPv6 в туннеле блокируется")}
  <h3>Правила маршрутизации</h3><p class="muted">Правила применяются сверху вниз. VPN-only обновления подписок всегда идут через VPN.</p>
  <div class="routing-rules">${s.rules
    .map(
      (r) =>
        `<fieldset data-rule="${r.id}"><legend>Правило</legend><label><input data-rule-enabled type="checkbox" ${r.enabled ? "checked" : ""}> Включено</label><input data-rule-name value="${e(r.name)}" placeholder="Название" maxlength="80" aria-label="Название правила"><select data-rule-kind aria-label="Тип правила">${options(
          [
            ["domain", "Домен"],
            ["suffix", "Суффикс домена"],
            ["keyword", "Ключевое слово"],
            ["cidr", "IP CIDR"],
          ],
          r.kind,
        )}</select><input data-rule-value value="${e(r.value)}" placeholder="example.com или 192.0.2.0/24" required aria-label="Значение правила"><select data-rule-target aria-label="Действие правила">${options(
          [
            ["proxy", "VPN"],
            ["direct", "Напрямую"],
            ["block", "Блокировать"],
          ],
          r.target,
        )}</select><button type="button" class="button subtle" data-rule-up="${r.id}" aria-label="Переместить правило выше">↑</button><button type="button" class="delete" data-rule-remove="${r.id}">Удалить правило</button></fieldset>`,
    )
    .join(
      "",
    )}</div><button type="button" class="button subtle" data-rule-add>Добавить правило</button>
  <h3>Разделение приложений</h3><label>Режим<select name="splitMode">${options(
    [
      ["bypass", "Выбранные приложения напрямую"],
      ["only", "Только выбранные приложения через VPN"],
    ],
    s.splitMode,
  )}</select></label><label>Абсолютные пути исполняемых файлов, по одному на строку<textarea name="splitApps" rows="3" spellcheck="false">${e(s.splitApps.join("\n"))}</textarea></label><p class="muted">Используются пути процессов этой ОС, а не Android package IDs. Пустой список отправляет все приложения через VPN. Процессы, которые ОС не смогла определить, следуют правилу по умолчанию выбранного режима.</p>
  <h3>Восстановление и проверки</h3>${check("autoReconnect", "Восстанавливать подключение после сбоя, сна или смены сети")}${check("healthEnabled", "Проверять доступность HTTPS через активный туннель")}${check("failover", "После повторных ошибок выбирать доступный сервер")}${check("autoConnect", "Подключаться к последнему серверу при запуске приложения")}
  <div class="network-grid"><label>URL HTTPS-проверки<input name="healthUrl" required value="${e(s.healthUrl)}"></label><label>Интервал<select name="healthIntervalSeconds">${options(
    [
      ["60", "1 минута"],
      ["120", "2 минуты"],
      ["300", "5 минут"],
      ["600", "10 минут"],
      ["900", "15 минут"],
    ],
    String(s.healthIntervalSeconds),
  )}</select></label></div>
  <h3>Интерфейс</h3><div class="network-grid"><label>Тема<select name="theme">${options(
    [
      ["system", "Системная"],
      ["light", "Светлая"],
      ["dark", "Тёмная"],
    ],
    s.theme,
  )}</select></label><label>Язык<select name="language">${options(
    [
      ["ru", "Русский"],
      ["en", "English"],
    ],
    s.language,
  )}</select></label></div>
  <p role="alert" class="dialog-error"></p><div class="dialog-actions"><button type="button" class="button subtle" data-settings-reset>Отменить изменения</button><button class="button">Сохранить настройки</button></div></form></article>`;
}
export function bindNetworkSettings(
  root: HTMLElement,
  state: Snapshot,
  update: (s: Snapshot) => void,
  render: () => void,
) {
  const form = root.querySelector<HTMLFormElement>("#network-settings");
  if (!form) return;
  const field = (name: string) =>
    form.elements.namedItem(name) as HTMLInputElement;
  const read = (): ConnectionSettings => ({
    mtu: Number(field("mtu").value),
    ipv6: field("ipv6").checked,
    dns: field("dns").value,
    bootstrapDns: field("bootstrapDns").value,
    splitMode: field("splitMode").value as "bypass" | "only",
    splitApps: field("splitApps")
      .value.split(/\r?\n/)
      .map((s) => s.trim())
      .filter(Boolean),
    healthEnabled: field("healthEnabled").checked,
    healthIntervalSeconds: Number(field("healthIntervalSeconds").value),
    healthUrl: field("healthUrl").value,
    autoReconnect: field("autoReconnect").checked,
    failover: field("failover").checked,
    autoConnect: field("autoConnect").checked,
    theme: field("theme").value as ConnectionSettings["theme"],
    language: field("language").value as ConnectionSettings["language"],
    rules: Array.from(form.querySelectorAll<HTMLElement>("[data-rule]")).map(
      (row) => ({
        id: row.dataset.rule!,
        name: row.querySelector<HTMLInputElement>("[data-rule-name]")!.value,
        enabled: row.querySelector<HTMLInputElement>("[data-rule-enabled]")!
          .checked,
        kind: row.querySelector<HTMLSelectElement>("[data-rule-kind]")!
          .value as RoutingRule["kind"],
        value: row.querySelector<HTMLInputElement>("[data-rule-value]")!.value,
        target: row.querySelector<HTMLSelectElement>("[data-rule-target]")!
          .value as RoutingRule["target"],
      }),
    ),
  });
  form.oninput = () => {
    draft = read();
  };
  form.onchange = () => {
    draft = read();
  };
  form.querySelector<HTMLButtonElement>("[data-rule-add]")!.onclick = () => {
    draft = read();
    draft.rules.push({
      id: crypto.randomUUID(),
      name: "",
      enabled: true,
      kind: "domain",
      value: "",
      target: "proxy",
    });
    render();
  };
  form.querySelectorAll<HTMLButtonElement>("[data-rule-remove]").forEach(
    (b) =>
      (b.onclick = () => {
        draft = read();
        draft.rules = draft.rules.filter((r) => r.id !== b.dataset.ruleRemove);
        render();
      }),
  );
  form.querySelectorAll<HTMLButtonElement>("[data-rule-up]").forEach(
    (b) =>
      (b.onclick = () => {
        draft = read();
        const i = draft.rules.findIndex((r) => r.id === b.dataset.ruleUp);
        if (i > 0)
          [draft.rules[i - 1], draft.rules[i]] = [
            draft.rules[i],
            draft.rules[i - 1],
          ];
        render();
      }),
  );
  form.querySelector<HTMLButtonElement>("[data-settings-reset]")!.onclick =
    () => {
      draft = null;
      render();
    };
  form.onsubmit = async (event) => {
    event.preventDefault();
    draft = read();
    try {
      const next = await call("save_connection_settings", { input: draft });
      draft = null;
      update(next);
    } catch (error) {
      form.querySelector("[role=alert]")!.textContent = String(error);
    }
  };
}
