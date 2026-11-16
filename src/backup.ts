import { invoke } from "@tauri-apps/api/core";
import { native, type Snapshot } from "./api";

type Preview = {
  token: string;
  summary: {
    servers: number;
    subscriptions: number;
    favorites: number;
    rules: number;
    warnings: string[];
  };
};
let undoToken: string | null = null;
export function backupHtml(): string {
  return `<article class="settings-card"><h2>Резервные копии и перенос Android</h2>
  <p class="muted">Копия содержит ссылки и пароли серверов. Экспорт защищён отдельным паролем. HWID этого компьютера не переносится. Восстановление не меняет активное подключение; подключитесь заново для применения настроек.</p>
  <form id="backup-form" class="network-form"><label>Пароль резервной копии<input name="backupPassword" type="password" maxlength="1024" autocomplete="off"></label>
  <button class="button subtle" type="button" data-backup-export>Скачать зашифрованную копию</button>
  <label>Копия Desktop или Android<input name="backupFile" type="file" accept=".json,application/json"></label>
  <button class="button subtle" type="submit">Предварительный просмотр</button>
  <p role="status" data-backup-status></p><p role="alert" data-backup-error></p></form>
  <button class="button subtle" data-backup-undo ${undoToken ? "" : "disabled"}>Отменить восстановление</button>
  <p class="muted">Отмена доступна 10 минут в этом сеансе, пока библиотека не изменилась.</p>
  <dialog id="backup-preview" aria-labelledby="backup-title"><h2 id="backup-title">Подтверждение восстановления</h2><p data-backup-counts></p><div data-backup-warnings></div>
  <label><input type="checkbox" data-backup-replace> Заменить текущую библиотеку и настройки</label><p class="muted">Без замены серверы и избранное объединяются; текущие настройки сохраняются. Передача HWID, расписание и автоподключение для перенесённых данных выключены.</p>
  <p role="alert" data-restore-error></p><div class="dialog-actions"><button class="button subtle" data-backup-cancel>Отмена</button><button class="button primary" data-backup-confirm>Восстановить</button></div></dialog></article>`;
}

export function bindBackup(
  root: HTMLElement,
  update: (next: Snapshot) => void,
): void {
  const form = root.querySelector<HTMLFormElement>("#backup-form");
  if (!form) return;
  const password = form.elements.namedItem(
    "backupPassword",
  ) as HTMLInputElement;
  const file = form.elements.namedItem("backupFile") as HTMLInputElement;
  const error = root.querySelector<HTMLElement>("[data-backup-error]")!;
  const status = root.querySelector<HTMLElement>("[data-backup-status]")!;
  const dialog = root.querySelector<HTMLDialogElement>("#backup-preview")!;
  let preview: Preview | null = null;
  let busy = false;
  const operation = async (action: () => Promise<void>) => {
    if (busy) return;
    busy = true;
    error.textContent = "";
    status.textContent = "Обработка резервной копии…";
    form
      .querySelectorAll<HTMLButtonElement>("button")
      .forEach((b) => (b.disabled = true));
    try {
      if (!native)
        throw new Error("Резервные копии доступны в настольном приложении.");
      await action();
    } catch (e) {
      error.textContent = String(e);
    } finally {
      password.value = "";
      file.value = "";
      status.textContent = "";
      busy = false;
      form
        .querySelectorAll<HTMLButtonElement>("button")
        .forEach((b) => (b.disabled = false));
    }
  };
  root.querySelector<HTMLButtonElement>("[data-backup-export]")!.onclick = () =>
    void operation(async () => {
      const encrypted = await invoke<string>("export_backup", {
        password: password.value,
      });
      const url = URL.createObjectURL(
        new Blob([encrypted], { type: "application/json" }),
      );
      const link = document.createElement("a");
      link.href = url;
      link.download = `Moth-backup-${new Date().toISOString().slice(0, 10)}.json`;
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
    });
  form.onsubmit = (e) => {
    e.preventDefault();
    void operation(async () => {
      const selected = file.files?.[0];
      if (!selected) throw new Error("Выберите резервную копию.");
      if (selected.size > 8 * 1024 * 1024)
        throw new Error("Резервная копия слишком большая");
      preview = await invoke<Preview>("preview_backup", {
        text: await selected.text(),
        password: password.value,
      });
      const s = preview.summary;
      root.querySelector("[data-backup-counts]")!.textContent =
        `Серверы: ${s.servers} · Подписки: ${s.subscriptions} · Избранное: ${s.favorites} · Правила: ${s.rules}`;
      const warnings = root.querySelector("[data-backup-warnings]")!;
      warnings.replaceChildren(
        ...s.warnings.map((warning) => {
          const p = document.createElement("p");
          p.textContent = warning;
          return p;
        }),
      );
      root.querySelector<HTMLInputElement>("[data-backup-replace]")!.checked =
        false;
      root.querySelector("[data-restore-error]")!.textContent = "";
      dialog.showModal();
    });
  };
  root.querySelector<HTMLButtonElement>("[data-backup-cancel]")!.onclick = () =>
    dialog.close();
  dialog.onclose = () => {
    preview = null;
    if (native) void invoke("discard_backup").catch(() => {});
  };
  root.querySelector<HTMLButtonElement>("[data-backup-confirm]")!.onclick =
    async () => {
      if (!preview || busy) return;
      busy = true;
      const button = root.querySelector<HTMLButtonElement>(
        "[data-backup-confirm]",
      )!;
      button.disabled = true;
      try {
        const result = await invoke<{ snapshot: Snapshot; undoToken: string }>(
          "confirm_backup",
          {
            token: preview.token,
            replace: root.querySelector<HTMLInputElement>(
              "[data-backup-replace]",
            )!.checked,
          },
        );
        undoToken = result.undoToken;
        dialog.close();
        update(result.snapshot);
      } catch (e) {
        root.querySelector("[data-restore-error]")!.textContent = String(e);
      } finally {
        busy = false;
        button.disabled = false;
      }
    };
  root.querySelector<HTMLButtonElement>("[data-backup-undo]")!.onclick = () =>
    void operation(async () => {
      const next = await invoke<Snapshot>("undo_backup", { token: undoToken });
      undoToken = null;
      update(next);
    });
}
