import { test, expect } from "@playwright/test";

test("an incompatible Clash policy needs an explicit nodes-only choice", async ({ page }) => {
  await page.addInitScript(() => {
    const host = window as unknown as {
      isTauri: boolean;
      __TAURI_INTERNALS__: unknown;
      imports: unknown[];
    };
    host.isTauri = true;
    host.imports = [];
    const snapshot = {
      profiles: [], selected: null, version: "0.3.6", engines: [],
      capabilities: { platform: "windows", secureStorage: true, systemTunnel: true, killSwitch: true },
    };
    host.__TAURI_INTERNALS__ = {
      invoke: async (command: string, args?: Record<string, unknown>) => {
        if (command === "update_monitor_mode") return false;
        if (command === "tunnel_status") return { phase: "disconnected", ownedByCaller: false, error: null };
        if (command === "preview_clash") {
          if (typeof args?.text !== "string" || !args.text.includes("synthetic-only")) throw "Missing document";
          return { token: "fixture", summary: {
            canApply: false, nodeOnlyCount: 1, nodes: [{name: "<img src=x>", kind: "http"}],
            groups: [], providers: [], rules: [], dnsMode: null, dnsFields: [], managedFields: [],
            issues: [{field: "external-controller", reason: "Unsupported field"}],
          } };
        }
        if (command === "apply_clash") host.imports.push(args);
        return snapshot;
      },
    };
  });
  await page.goto("/");
  await page.locator('[data-page="servers"]').first().click();
  await page.locator("[data-clash-file]").setInputFiles({
    name: "fixture.yaml", mimeType: "text/yaml", buffer: Buffer.from("proxies: # synthetic-only"),
  });
  await page.locator("[data-clash-file-form] button").click();
  const dialog = page.getByRole("dialog", { name: "Предпросмотр Clash" });
  await expect(dialog.getByText("external-controller", { exact: true })).toBeVisible();
  await expect(dialog.getByRole("button", { name: "Сохранить профиль целиком" })).toBeDisabled();
  await expect(dialog.locator("img")).toHaveCount(0);
  await expect(dialog.getByText("synthetic-only")).toHaveCount(0);
  expect(await page.evaluate(() => (window as unknown as { imports: unknown[] }).imports)).toEqual([]);
  await dialog.getByRole("button", { name: "Только совместимые узлы" }).click();
  await expect(dialog).not.toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { imports: unknown[] }).imports)).toEqual([
    { token: "fixture", name: "Clash", nodesOnly: true },
  ]);
});
