import { call, deviceId, type Snapshot, type Profile } from "./api";

const escape = (s: string) =>
  s.replace(
    /[&<>"']/g,
    (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        c
      ]!,
  );
let query = "";
let sort = "source";
let onlyFavorites = false;
const pendingRefresh = new Set<string>();
type Context = {
  busy: boolean;
  active: boolean;
  probingId: string | null;
  results: Map<string, string>;
  icon: string;
};
const bytes = (n: number | null) =>
  n === null ? "—" : `${(n / 1024 / 1024 / 1024).toFixed(2)} ГБ`;
const date = (n: number | null) =>
  n ? new Date(n * 1000).toLocaleString() : "—";

export function libraryHtml(state: Snapshot, context: Context): string {
  const sources = state.subscriptions || [];
  const favorites = new Set(state.favorites || []);
  const visible = state.profiles.filter(
    (p) =>
      (!onlyFavorites || favorites.has(p.id)) &&
      `${p.name} ${p.host} ${p.protocol}`
        .toLocaleLowerCase()
        .includes(query.toLocaleLowerCase()),
  );
  if (sort === "name") visible.sort((a, b) => a.name.localeCompare(b.name));
  if (sort === "latency")
    visible.sort(
      (a, b) =>
        latency(context.results.get(a.id)) - latency(context.results.get(b.id)),
    );
  const row = (p: Profile) =>
    `<article class="server-row ${p.id === state.selected ? "chosen" : ""}"><div class="server-symbol">${context.icon}</div><div class="server-info"><h3>${escape(p.name)}</h3><p>${escape(p.host)}:${p.port}</p><div class="tags"><span>${escape(p.protocol)}</span><span>${escape(p.transport)}</span><span>${escape(p.security)}</span></div></div><div class="server-actions"><button class="button subtle" data-favorite="${p.id}" aria-pressed="${favorites.has(p.id)}" aria-label="${favorites.has(p.id) ? "Убрать из избранного" : "В избранное"}">${favorites.has(p.id) ? "★" : "☆"}</button><button class="button subtle" data-probe="${p.id}" ${context.busy || context.probingId ? "disabled" : ""}>${context.probingId === p.id ? "Проверяем…" : "Проверить сервер"}</button><span class="probe-result" role="status">${escape(context.results.get(p.id) || "")}</span><button class="button subtle" data-select="${p.id}" ${context.busy || context.active ? "disabled" : ""}>${p.id === state.selected ? "Выбран" : "Выбрать"}</button><button class="delete" data-delete="${p.id}" ${context.busy || context.active ? "disabled" : ""}>Удалить</button></div></article>`;
  const owned = new Set(sources.flatMap((s) => s.profiles));
  const archived = new Set(state.archived || []);
  const groups = [
    {
      name: "Избранное",
      profiles: visible.filter(
        (p) => favorites.has(p.id) && !archived.has(p.id),
      ),
    },
    {
      name: "Локальные серверы",
      profiles: visible.filter(
        (p) => !owned.has(p.id) && !favorites.has(p.id) && !archived.has(p.id),
      ),
    },
    {
      name: "Сохранённые из прежнего списка",
      profiles: visible.filter((p) => archived.has(p.id)),
    },
    ...sources.map((s) => ({
      name: s.name,
      profiles: visible.filter(
        (p) => s.profiles.includes(p.id) && !favorites.has(p.id),
      ),
    })),
  ];
  return `<div class="page-heading"><div><p class="eyebrow">ПРОФИЛИ</p><h1>Серверы</h1><p class="muted">Ссылки серверов и подписки провайдеров</p></div><div class="library-buttons"><button class="button" data-import>Добавить сервер</button><button class="button" data-source-add>Добавить подписку</button><button class="button subtle" data-hwid>HWID устройства</button></div></div>
  <section class="subscription-list" aria-label="Подписки">${sources.map((s) => `<article class="settings-card"><div class="card-top"><h2>${escape(s.name)}</h2><div class="library-buttons"><button class="button subtle" data-source-refresh="${s.id}" ${context.busy || pendingRefresh.size ? "disabled" : ""}>${pendingRefresh.has(s.id) ? "Обновляем…" : "Обновить"}</button><button class="button subtle" data-source-edit="${s.id}">Изменить</button><button class="delete" data-source-delete="${s.id}" ${context.active && s.profiles.includes(state.selected || "") ? "disabled" : ""}>Удалить</button></div></div><p class="muted">Серверов: ${s.profiles.length} · Обновлено: ${date(s.lastSuccess)}${s.refreshHours ? ` · Каждые ${s.refreshHours} ч при работающем приложении` : " · Вручную"}${s.vpnOnly ? " · Только через VPN" : ""}</p><p class="muted">Трафик: ${bytes(s.quota.usedBytes)} / ${bytes(s.quota.totalBytes)} · Срок: ${date(s.quota.expires)}</p>${s.lastError ? `<p class="error" role="alert">${escape(s.lastError)}</p>` : ""}</article>`).join("")}</section>
  <div class="library-toolbar"><label>Поиск <input id="server-search" type="search" value="${escape(query)}" placeholder="Название, адрес или протокол"></label><label>Сортировка <select id="server-sort">${[
    ["source", "По подписке"],
    ["name", "По названию"],
    ["latency", "По задержке"],
  ]
    .map(
      ([v, n]) =>
        `<option value="${v}" ${sort === v ? "selected" : ""}>${n}</option>`,
    )
    .join(
      "",
    )}</select></label><label><input id="favorites-only" type="checkbox" ${onlyFavorites ? "checked" : ""}> Избранное</label><button class="button subtle" data-probe-all ${context.busy || context.probingId || !visible.length ? "disabled" : ""}>Проверить видимые</button><button class="button subtle" data-probe-cancel hidden>Отменить проверку</button></div>
  ${
    state.profiles.length
      ? groups
          .filter((g) => g.profiles.length)
          .map(
            (g) =>
              `<section class="server-group"><h2>${escape(g.name)}</h2><div class="server-list">${g.profiles.map(row).join("")}</div></section>`,
          )
          .join("") || '<p class="muted">Ничего не найдено.</p>'
      : '<div class="empty-state"><h2>Нет серверов</h2><p>Добавьте ссылку сервера или подписку провайдера.</p><button class="button" data-import>Добавить первый сервер</button></div>'
  }
  <dialog id="source-dialog"><form id="source-form"><h2>Подписка</h2><label>Название<input name="name" required maxlength="80" autocomplete="off"></label><label>Ссылка подписки<input name="url" type="password" autocomplete="off" spellcheck="false" placeholder="https://…"></label><p class="muted">Ссылка хранится в защищённом хранилище. При редактировании оставьте поле пустым, чтобы сохранить прежнюю.</p><label><input type="checkbox" name="allowHttp"> Разрешить HTTP: секретная ссылка и ответ передаются без шифрования</label><label><input type="checkbox" name="sendHwid"> Отправлять HWID этому провайдеру</label><label><input type="checkbox" name="sendDeviceInfo"> Также отправлять сведения об ОС и типе устройства</label><p class="muted">Данные устройства отправляются только по HTTPS в пределах исходного origin. Лимиты и регистрацию устройства определяет провайдер.</p><label>User-Agent<input name="userAgent" required maxlength="160" autocomplete="off"></label><label>Обновление<select name="refreshHours"><option value="0">Вручную</option><option value="6">Каждые 6 часов</option><option value="12">Каждые 12 часов</option><option value="24">Раз в сутки</option></select></label><label><input type="checkbox" name="vpnOnly"> Обновлять только через мой активный VPN</label><p class="dialog-error" role="alert"></p><div class="dialog-actions"><button type="button" data-source-close class="button subtle">Отмена</button><button class="button">Сохранить</button></div></form></dialog>
  <dialog id="hwid-dialog"><h2>HWID этого устройства</h2><p>Стабильный случайный идентификатор этой установки Moth. Зарегистрируйте его у провайдера, если требуется привязка устройства. Идентификатор телефона не используется.</p><input id="device-id" readonly aria-label="HWID"><div class="dialog-actions"><button class="button subtle" data-hwid-close>Закрыть</button><button class="button" data-hwid-copy>Скопировать</button></div><p role="status" id="hwid-message"></p></dialog>
  <dialog id="source-delete"><form method="dialog"><h2>Удалить подписку и её серверы?</h2><p>У провайдера подписка продолжит действовать.</p><div class="dialog-actions"><button value="cancel" class="button subtle">Отмена</button><button value="delete" class="button">Удалить</button></div></form></dialog>`;
}
function latency(s?: string): number {
  return Number(s?.match(/· (\d+) мс/)?.[1] || Infinity);
}

export function bindLibrary(
  root: HTMLElement,
  state: Snapshot,
  update: (next: Snapshot) => void,
  rerender: () => void,
  failure: (e: unknown) => void,
) {
  const search = root.querySelector<HTMLInputElement>("#server-search");
  if (!search) return;
  search.oninput = () => {
    const cursor = search.selectionStart;
    query = search.value;
    rerender();
    const next = root.querySelector<HTMLInputElement>("#server-search")!;
    next.focus();
    if (cursor !== null) next.setSelectionRange(cursor, cursor);
  };
  root.querySelector<HTMLSelectElement>("#server-sort")!.onchange = (e) => {
    sort = (e.target as HTMLSelectElement).value;
    rerender();
  };
  root.querySelector<HTMLInputElement>("#favorites-only")!.onchange = (e) => {
    onlyFavorites = (e.target as HTMLInputElement).checked;
    rerender();
  };
  root.querySelectorAll<HTMLButtonElement>("[data-favorite]").forEach(
    (b) =>
      (b.onclick = () => {
        void call("toggle_favorite", { id: b.dataset.favorite })
          .then(update)
          .catch(failure);
      }),
  );
  const dialog = root.querySelector<HTMLDialogElement>("#source-dialog")!;
  const form = dialog.querySelector<HTMLFormElement>("form")!;
  let editing: string | null = null;
  const field = (name: string) =>
    form.elements.namedItem(name) as HTMLInputElement;
  const open = (id: string | null) => {
    editing = id;
    form.reset();
    field("userAgent").value = `Moth/${state.version}`;
    const s = (state.subscriptions || []).find((s) => s.id === id);
    if (s) {
      for (const key of ["name", "userAgent", "refreshHours"] as const)
        field(key).value = String(s[key]);
      for (const key of [
        "sendHwid",
        "sendDeviceInfo",
        "vpnOnly",
        "allowHttp",
      ] as const)
        field(key).checked = s[key];
    }
    field("url").required = !s;
    dialog.showModal();
  };
  root.querySelector<HTMLButtonElement>("[data-source-add]")!.onclick = () =>
    open(null);
  root
    .querySelectorAll<HTMLButtonElement>("[data-source-edit]")
    .forEach((b) => (b.onclick = () => open(b.dataset.sourceEdit!)));
  root.querySelector<HTMLButtonElement>("[data-source-close]")!.onclick = () =>
    dialog.close();
  dialog.onclose = () => {
    form.reset();
    dialog.querySelector(".dialog-error")!.textContent = "";
    editing = null;
  };
  const refresh = async (id: string) => {
    pendingRefresh.add(id);
    rerender();
    try {
      update(await call("refresh_subscription", { id }));
    } catch (error) {
      try {
        update(await call("get_snapshot"));
      } catch {
        /* Existing error is sufficient. */
      }
      failure(error);
    } finally {
      pendingRefresh.delete(id);
      rerender();
    }
  };
  form.onsubmit = async (e) => {
    e.preventDefault();
    const input = {
      id: editing,
      name: field("name").value,
      url: field("url").value,
      userAgent: field("userAgent").value,
      refreshHours: Number(field("refreshHours").value),
      sendHwid: field("sendHwid").checked,
      sendDeviceInfo: field("sendDeviceInfo").checked,
      vpnOnly: field("vpnOnly").checked,
      allowHttp: field("allowHttp").checked,
    };
    try {
      const next = await call("save_subscription", { input });
      const created = !input.id
        ? next.subscriptions?.find(
            (s) => !(state.subscriptions || []).some((old) => old.id === s.id),
          )?.id
        : undefined;
      dialog.close();
      update(next);
      if (created) void refresh(created);
    } catch (e) {
      dialog.querySelector(".dialog-error")!.textContent = String(e);
    }
  };
  root.querySelectorAll<HTMLButtonElement>("[data-source-refresh]").forEach(
    (b) =>
      (b.onclick = () => {
        void refresh(b.dataset.sourceRefresh!);
      }),
  );
  const deletion = root.querySelector<HTMLDialogElement>("#source-delete")!;
  let removing: string | null = null;
  root.querySelectorAll<HTMLButtonElement>("[data-source-delete]").forEach(
    (b) =>
      (b.onclick = () => {
        removing = b.dataset.sourceDelete!;
        deletion.showModal();
      }),
  );
  deletion.onclose = () => {
    const id = removing;
    removing = null;
    if (id && deletion.returnValue === "delete")
      void call("delete_subscription", { id }).then(update).catch(failure);
  };
  const hw = root.querySelector<HTMLDialogElement>("#hwid-dialog")!;
  const value = hw.querySelector<HTMLInputElement>("input")!;
  root.querySelector<HTMLButtonElement>("[data-hwid]")!.onclick = async () => {
    try {
      value.value = await deviceId();
      hw.showModal();
    } catch (e) {
      failure(e);
    }
  };
  root.querySelector<HTMLButtonElement>("[data-hwid-close]")!.onclick = () =>
    hw.close();
  hw.onclose = () => {
    value.value = "";
    hw.querySelector("#hwid-message")!.textContent = "";
  };
  root.querySelector<HTMLButtonElement>("[data-hwid-copy]")!.onclick =
    async () => {
      try {
        await navigator.clipboard.writeText(value.value);
        hw.querySelector("#hwid-message")!.textContent = "Скопировано";
      } catch {
        hw.querySelector("#hwid-message")!.textContent =
          "Выделите HWID и скопируйте вручную";
      }
    };
}
