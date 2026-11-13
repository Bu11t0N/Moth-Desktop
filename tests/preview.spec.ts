import { test, expect } from "@playwright/test";

test("preview never claims protection; navigation and import failure are honest", async ({
  page,
}) => {
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Твоя сторона сети." }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Подключить" })).toBeDisabled();
  await expect(page.getByText("VPN сейчас не защищает трафик")).toBeVisible();
  await page.screenshot({ path: "test-results/home.png", fullPage: true });
  await page.getByRole("button", { name: "Серверы 0" }).click();
  await expect(
    page.getByRole("heading", { name: "Серверы", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Добавить первый сервер" }).click();
  await page
    .getByLabel("Ссылки серверов")
    .fill("trojan://secret@example.com:443");
  await page.getByRole("button", { name: "Импортировать" }).click();
  await expect(
    page
      .getByRole("alert")
      .filter({ hasText: "Импорт доступен в приложении Windows" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Отмена", exact: true }).click();
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await expect(page.getByText("Недоступно в этом режиме")).toBeVisible();
});

test("native adapter renders untrusted summaries as text and manages selected server", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as {
      isTauri: boolean;
      __TAURI_INTERNALS__: unknown;
    };
    w.isTauri = true;
    let profiles: object[] = [];
    w.__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => {
        if (cmd === "import_servers")
          profiles = [
            {
              id: "test-id",
              name: "<img src=x onerror=alert(1)>",
              host: "example.com",
              port: 443,
              protocol: "Trojan",
              transport: "TCP",
              security: "TLS",
            },
          ];
        if (cmd === "remove_server") profiles = [];
        return {
          profiles,
          selected: profiles.length ? "test-id" : null,
          version: "0.3.1",
          capabilities: {
            platform: "windows",
            secureStorage: true,
            systemTunnel: false,
            killSwitch: false,
          },
        };
      },
    };
  });
  await page.goto("/");
  await page
    .getByRole("button", { name: "Добавить сервер", exact: true })
    .click();
  await page
    .getByLabel("Ссылки серверов")
    .fill("trojan://example-secret@example.com:443");
  await page.getByRole("button", { name: "Импортировать" }).click();
  await expect(
    page.getByRole("heading", { name: "<img src=x onerror=alert(1)>" }),
  ).toBeVisible();
  await expect(page.locator(".server-row img")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Выбран", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Удалить", exact: true }).click();
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Удалить", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Здесь начнётся твой маршрут" }),
  ).toBeVisible();
});

test("keyboard closes import and no external resources are requested", async ({
  page,
}) => {
  const external: string[] = [];
  page.on("request", (r) => {
    if (!r.url().startsWith("http://127.0.0.1:1420")) external.push(r.url());
  });
  await page.goto("/");
  await page
    .getByRole("button", { name: "Добавить сервер", exact: true })
    .click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.getByLabel("Ссылки серверов").fill("secret");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).not.toBeVisible();
  await page
    .getByRole("button", { name: "Добавить сервер", exact: true })
    .click();
  await expect(page.getByLabel("Ссылки серверов")).toHaveValue("");
  expect(external).toEqual([]);
});

test("server probe reports measured result without enabling system VPN", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as {
      isTauri: boolean;
      __TAURI_INTERNALS__: unknown;
    };
    w.isTauri = true;
    w.__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => {
        if (cmd === "probe_server") {
          await new Promise((resolve) => setTimeout(resolve, 100));
          return { profileId: "probe-id", latencyMs: 125 };
        }
        return {
          profiles: [
            {
              id: "probe-id",
              name: "Тестовый сервер",
              host: "example.com",
              port: 443,
              protocol: "VLESS",
              transport: "TCP",
              security: "TLS",
            },
          ],
          selected: "probe-id",
          version: "0.3.1",
          engines: [
            {
              name: "xray",
              version: "26.10.21",
              available: true,
              message: null,
            },
          ],
          capabilities: {
            platform: "windows",
            secureStorage: true,
            systemTunnel: false,
            killSwitch: false,
          },
        };
      },
    };
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Серверы 1" }).click();
  await page
    .getByRole("button", { name: "Проверить сервер", exact: true })
    .click();
  await expect(page.getByRole("status")).toHaveText("HTTPS · 125 мс");
  await page.screenshot({ path: "test-results/servers.png", fullPage: true });
  await page.getByRole("button", { name: "Главная", exact: true }).click();
  await expect(page.getByText("VPN сейчас не защищает трафик")).toBeVisible();
  await expect(page.getByRole("button", { name: "Подключить" })).toBeDisabled();
});

test("authenticated service health is separate from VPN protection", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as {
      isTauri: boolean;
      __TAURI_INTERNALS__: unknown;
    };
    w.isTauri = true;
    w.__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => {
        if (cmd === "service_status")
          return {
            version: "0.3.1",
            protocol: 1,
            systemTunnel: false,
            engines: [],
          };
        return {
          profiles: [],
          selected: null,
          version: "0.3.1",
          engines: [],
          capabilities: {
            platform: "windows",
            secureStorage: true,
            systemTunnel: false,
            killSwitch: false,
          },
        };
      },
    };
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await page
    .getByRole("button", { name: "Проверить связь со службой", exact: true })
    .click();
  await expect(page.getByRole("status")).toContainText(
    "Связь установлена · 0.3.1",
  );
  await page.screenshot({ path: "test-results/service.png", fullPage: true });
  await page.getByRole("button", { name: "Главная", exact: true }).click();
  await expect(page.getByText("VPN сейчас не защищает трафик")).toBeVisible();
});

test("VPN follows service transitions and loses connected state when service disappears", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as {
      isTauri: boolean;
      __TAURI_INTERNALS__: unknown;
      loseService: () => void;
    };
    w.isTauri = true;
    let phase = "disconnected";
    let unavailable = false;
    w.loseService = () => {
      unavailable = true;
    };
    w.__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => {
        if (
          cmd === "tunnel_status" ||
          cmd === "connect_vpn" ||
          cmd === "disconnect_vpn"
        ) {
          if (unavailable) throw new Error("Служба недоступна");
          if (cmd === "connect_vpn") {
            phase = "connecting";
            await new Promise((r) => setTimeout(r, 200));
            phase = "connected";
          }
          if (cmd === "disconnect_vpn") phase = "disconnected";
          return {
            phase,
            ownedByCaller: phase !== "disconnected",
            error: null,
          };
        }
        return {
          profiles: [
            {
              id: "vpn",
              name: "Мой сервер",
              host: "example.com",
              port: 443,
              protocol: "VLESS",
              transport: "TCP",
              security: "TLS",
            },
          ],
          selected: "vpn",
          version: "0.3.1",
          engines: [],
          capabilities: {
            platform: "windows",
            secureStorage: true,
            systemTunnel: true,
            killSwitch: false,
          },
        };
      },
    };
  });
  await page.goto("/");
  await expect(
    page.getByRole("button", { name: "Подключить", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Подключить", exact: true }).click();
  await expect(page.getByText("VPN сейчас не защищает трафик")).toBeVisible();
  await expect(
    page.getByText("Туннель проверен · kill switch пока недоступен"),
  ).toBeVisible();
  await page.getByRole("button", { name: "Отключить", exact: true }).click();
  await expect(page.getByText("VPN сейчас не защищает трафик")).toBeVisible();
  await page.getByRole("button", { name: "Подключить", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Отключить", exact: true }),
  ).toBeEnabled();
  await page.screenshot({ path: "test-results/vpn.png", fullPage: true });
  await page.evaluate(() =>
    (window as unknown as { loseService: () => void }).loseService(),
  );
  await expect(page.getByText("VPN сейчас не защищает трафик")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Подключить", exact: true }),
  ).toBeDisabled();
});

test("owner can cancel a connecting operation without a stale connected state", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as {
      isTauri: boolean;
      __TAURI_INTERNALS__: unknown;
    };
    w.isTauri = true;
    let phase = "disconnected";
    let finish: (() => void) | undefined;
    w.__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => {
        if (["tunnel_status", "connect_vpn", "disconnect_vpn"].includes(cmd)) {
          if (cmd === "connect_vpn") {
            phase = "connecting";
            await new Promise<void>((r) => {
              finish = r;
            });
          }
          if (cmd === "disconnect_vpn") {
            phase = "disconnected";
            finish?.();
          }
          return {
            phase,
            ownedByCaller: phase !== "disconnected",
            error: null,
          };
        }
        return {
          profiles: [
            {
              id: "vpn",
              name: "Сервер",
              host: "example.com",
              port: 443,
              protocol: "VLESS",
              transport: "TCP",
              security: "TLS",
            },
          ],
          selected: "vpn",
          version: "0.3.1",
          engines: [],
          capabilities: {
            platform: "windows",
            secureStorage: true,
            systemTunnel: true,
            killSwitch: false,
          },
        };
      },
    };
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Подключить", exact: true }).click();
  await page
    .getByRole("button", { name: "Отменить подключение", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Подключить", exact: true }),
  ).toBeEnabled();
  await expect(page.getByText("VPN сейчас не защищает трафик")).toBeVisible();
});
