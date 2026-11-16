import { test, expect } from "@playwright/test";
test("saved English and light theme apply without translating profile names", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as {
      isTauri: boolean;
      __TAURI_INTERNALS__: unknown;
    };
    w.isTauri = true;
    const settings = {
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
      theme: "light",
      language: "en",
    };
    const state = {
      profiles: [
        {
          id: "fixture",
          name: "Подключить",
          host: "example.invalid",
          port: 443,
          protocol: "Trojan",
          transport: "TCP",
          security: "TLS",
        },
      ],
      selected: "fixture",
      subscriptions: [],
      favorites: [],
      version: "0.3.4",
      settings,
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
        if (command === "service_status")
          return {
            version: "0.3.4",
            protocol: 2,
            systemTunnel: true,
            engines: [],
          };
        return state;
      },
    };
  });
  await page.goto("/");
  await expect(page.locator("html")).toHaveAttribute("lang", "en");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expect(page.locator(".selected-name")).toHaveText("Подключить");
  await page.locator('[data-page="settings"]').first().click();
  await expect(
    page.getByRole("heading", {
      name: "Network and connection behavior",
      exact: true,
    }),
  ).toBeVisible();
  const remaining = await page.locator("main").innerText();
  expect(remaining.replace("Русский", "")).not.toMatch(/[А-Яа-яЁё]/);
  await page.screenshot({
    path: test.info().outputPath("light-settings.png"),
    fullPage: true,
  });
});
