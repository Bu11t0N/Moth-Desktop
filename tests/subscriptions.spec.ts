import { test, expect } from "@playwright/test";

test("subscription consent defaults, protected edit and HWID are explicit", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as {
      isTauri: boolean;
      __TAURI_INTERNALS__: unknown;
    };
    w.isTauri = true;
    const source = {
      id: "source-fixture",
      name: "Provider",
      profiles: [],
      sendHwid: false,
      sendDeviceInfo: false,
      userAgent: "Moth/0.3.4",
      refreshHours: 0,
      vpnOnly: false,
      allowHttp: false,
      lastAttempt: null,
      lastSuccess: null,
      lastError: null,
      quota: { usedBytes: null, totalBytes: null, expires: null },
    };
    const snapshot = {
      profiles: [],
      subscriptions: [source],
      favorites: [],
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
      invoke: async (
        command: string,
        args?: { input?: typeof source & { url: string } },
      ) => {
        if (command === "update_monitor_mode") return false;
        if (command === "get_device_id") return "synthetic-installation-id";
        if (command === "tunnel_status")
          return { phase: "disconnected", ownedByCaller: false, error: null };
        if (command === "save_subscription") {
          if (args?.input?.sendHwid || args?.input?.sendDeviceInfo)
            throw "Consent unexpectedly enabled";
          if (args?.input?.id && args.input.url)
            throw "Stored URL must not be returned to edit field";
          return snapshot;
        }
        return snapshot;
      },
    };
  });
  await page.goto("/");
  await page.locator('[data-page="servers"]').first().click();
  await page
    .getByRole("button", { name: "Добавить подписку", exact: true })
    .click();
  const dialog = page.locator("#source-dialog");
  await expect(dialog.locator('[name="sendHwid"]')).not.toBeChecked();
  await expect(dialog.locator('[name="sendDeviceInfo"]')).not.toBeChecked();
  await expect(dialog.locator('[name="allowHttp"]')).not.toBeChecked();
  await dialog.getByRole("button", { name: "Отмена", exact: true }).click();
  await page.getByRole("button", { name: "Изменить", exact: true }).click();
  await expect(dialog.locator('[name="url"]')).toHaveValue("");
  await dialog.getByRole("button", { name: "Сохранить", exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await page
    .getByRole("button", { name: "HWID устройства", exact: true })
    .click();
  await expect(
    page.getByRole("textbox", { name: "HWID", exact: true }),
  ).toHaveValue("synthetic-installation-id");
  await page
    .locator("#hwid-dialog")
    .getByRole("button", { name: "Закрыть", exact: true })
    .click();
  await expect(page.locator("#device-id")).toHaveValue("");
});

test("search and favorites change visible groups without deleting profiles", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as {
      isTauri: boolean;
      __TAURI_INTERNALS__: unknown;
    };
    w.isTauri = true;
    const profiles = [
      {
        id: "a",
        name: "Alpha",
        host: "alpha.invalid",
        port: 443,
        protocol: "Trojan",
        transport: "TCP",
        security: "TLS",
      },
      {
        id: "b",
        name: "Beta",
        host: "beta.invalid",
        port: 443,
        protocol: "VLESS",
        transport: "TCP",
        security: "Reality",
      },
    ];
    const state = {
      profiles,
      subscriptions: [],
      favorites: ["b"],
      selected: "a",
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
      invoke: async (command: string) => {
        if (command === "update_monitor_mode") return false;
        if (command === "tunnel_status")
          return { phase: "disconnected", ownedByCaller: false, error: null };
        return state;
      },
    };
  });
  await page.goto("/");
  await page.locator('[data-page="servers"]').first().click();
  await expect(page.locator(".server-row")).toHaveCount(2);
  await page.locator("#server-search").fill("Alpha");
  await expect(page.locator(".server-row")).toHaveCount(1);
  await page.locator("#server-search").fill("");
  await page.locator("#favorites-only").check();
  await expect(page.locator(".server-row")).toHaveCount(1);
  await expect(page.locator(".server-row h3")).toHaveText("Beta");
  await page.locator("#favorites-only").uncheck();
  await expect(page.locator(".server-row")).toHaveCount(2);
});
