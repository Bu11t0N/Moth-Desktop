import { test, expect } from "@playwright/test";

test("backup preview clears input secrets and requires explicit confirmation", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as {
      isTauri: boolean;
      __TAURI_INTERNALS__: unknown;
      confirmations: number;
    };
    w.isTauri = true;
    w.confirmations = 0;
    const snapshot = {
      profiles: [],
      selected: null,
      version: "0.3.4",
      engines: [],
      capabilities: {
        platform: "windows",
        secureStorage: true,
        systemTunnel: true,
        killSwitch: false,
      },
    };
    w.__TAURI_INTERNALS__ = {
      invoke: async (command: string, args?: Record<string, unknown>) => {
        if (command === "update_monitor_mode") return false;
        if (command === "tunnel_status")
          return { phase: "disconnected", ownedByCaller: false, error: null };
        if (command === "preview_backup") {
          if (args?.password !== "synthetic-password") throw "Password missing";
          return {
            token: "opaque-fixture",
            summary: {
              servers: 2,
              subscriptions: 1,
              favorites: 1,
              rules: 1,
              warnings: ["Synthetic warning <script>unsafe()</script>"],
            },
          };
        }
        if (command === "confirm_backup") {
          if (args?.replace !== false) throw "Replacement must be explicit";
          w.confirmations++;
          return { snapshot, undoToken: "undo-fixture" };
        }
        return snapshot;
      },
    };
  });
  await page.goto("/");
  await page.locator('[data-page="settings"]').first().click();
  await page
    .getByLabel("Пароль резервной копии", { exact: true })
    .fill("synthetic-password");
  await page
    .getByLabel("Копия Desktop или Android", { exact: true })
    .setInputFiles({
      name: "synthetic.json",
      mimeType: "application/json",
      buffer: Buffer.from("{}"),
    });
  await page
    .getByRole("button", { name: "Предварительный просмотр", exact: true })
    .click();
  const dialog = page.locator("#backup-preview");
  await expect(dialog).toBeVisible();
  await expect(
    page.getByLabel("Пароль резервной копии", { exact: true }),
  ).toHaveValue("");
  await expect(
    page.getByLabel("Копия Desktop или Android", { exact: true }),
  ).toHaveValue("");
  await expect(dialog.locator("[data-backup-replace]")).not.toBeChecked();
  await expect(dialog.locator("script")).toHaveCount(0);
  expect(
    await page.evaluate(
      () => (window as unknown as { confirmations: number }).confirmations,
    ),
  ).toBe(0);
  await dialog
    .getByRole("button", { name: "Восстановить", exact: true })
    .click();
  await expect(dialog).not.toBeVisible();
  expect(
    await page.evaluate(
      () => (window as unknown as { confirmations: number }).confirmations,
    ),
  ).toBe(1);
  await expect(
    page.getByRole("button", { name: "Отменить восстановление", exact: true }),
  ).toBeEnabled();
});
