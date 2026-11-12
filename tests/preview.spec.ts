import { test, expect } from "@playwright/test";

test("preview never claims protection; navigation and import failure are honest", async ({
  page,
}) => {
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Твоя сторона сети." }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Подключение в разработке" }),
  ).toBeDisabled();
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
          version: "0.1.0",
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
