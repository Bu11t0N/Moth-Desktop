import { test, expect } from "@playwright/test";

test("updater renders signed metadata as text and offers no installation after signature failure",async ({page})=>{
  await page.addInitScript(()=>{
    const target=window as unknown as {isTauri:boolean;__TAURI_INTERNALS__:unknown};
    target.isTauri=true;
    target.__TAURI_INTERNALS__={invoke:async(command:string)=>{
      if(command==="update_monitor_mode")return false;
      if(command==="check_updates")return {phase:"available",update:{version:"0.3.4",notes:"<img src=x onerror=alert(1)>",file:"fixture",size:64},downloaded:0,total:64,error:null};
      if(command==="download_update")throw "Подпись обновления не прошла проверку";
      if(command==="tunnel_status")return {phase:"disconnected",ownedByCaller:false,error:null};
      return {profiles:[],selected:null,version:"0.3.4",engines:[],capabilities:{platform:"windows",secureStorage:true,systemTunnel:true,killSwitch:false}};
    }};
  });
  await page.goto("/");
  await page.getByRole("button",{name:"Настройки",exact:true}).click();
  await page.getByRole("button",{name:"Проверить обновления",exact:true}).click();
  await expect(page.getByText("<img src=x onerror=alert(1)>",{exact:true})).toBeVisible();
  await expect(page.locator(".update-card img")).toHaveCount(0);
  await page.getByRole("button",{name:"Скачать обновление",exact:true}).click();
  await expect(page.getByRole("alert")).toContainText("Подпись обновления не прошла проверку");
  await expect(page.getByRole("button",{name:"Установить обновление",exact:true})).toHaveCount(0);
  await expect(page.getByRole("button",{name:"Подключить",exact:true})).toHaveCount(0);
});

test("preview never claims protection; navigation and import failure are honest", async ({
  page,
}) => {
  await page.goto("/");
  await expect(
    page.getByRole("heading", { name: "Подключение VPN" }),
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
      .filter({ hasText: "Импорт доступен в настольном приложении" }),
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
          version: "0.3.4",
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
    page.getByRole("heading", { name: "Нет серверов" }),
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
          version: "0.3.4",
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
            version: "0.3.4",
            protocol: 2,
            systemTunnel: false,
            engines: [{ name: "xray", version: "26.10.21", available: false }],
          };
        return {
          profiles: [],
          selected: null,
          version: "0.3.4",
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
  await expect(page.getByRole("status", { name: "Состояние службы" })).toContainText(
    "Связь установлена · 0.3.4",
  );
  await expect(page.getByText("Совместим · IPC 2")).toBeVisible();
  await expect(page.getByText("26.10.21 · Недоступно")).toBeVisible();
  await expect(page.getByText("Не подтверждён", { exact: true })).toBeVisible();
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
          version: "0.3.4",
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
          version: "0.3.4",
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

test("diagnostics support IPC 2 service 0.3.1 and clear stale health after failure", async ({
  page,
}) => {
  await page.addInitScript(() => {
    const w = window as unknown as {
      isTauri: boolean;
      __TAURI_INTERNALS__: unknown;
    };
    w.isTauri = true;
    let checks = 0;
    w.__TAURI_INTERNALS__ = {
      invoke: async (cmd: string) => {
        if (cmd === "service_status") {
          if (++checks > 1) throw new Error("Служба недоступна");
          return {
            version: "0.3.1",
            protocol: 2,
            systemTunnel: true,
            engines: [
              { name: "xray", version: "26.10.21", available: true },
              { name: "sing-box", version: "1.12.25", available: true },
            ],
          };
        }
        if (cmd === "tunnel_status")
          return { phase: "disconnected", ownedByCaller: false, error: null };
        return {
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
      },
    };
  });
  await page.goto("/");
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  const refresh = page.getByRole("button", {
    name: "Проверить связь со службой",
    exact: true,
  });
  await refresh.click();
  await expect(page.getByRole("status", { name: "Состояние службы" })).toContainText(
    "Связь установлена · 0.3.1",
  );
  await expect(
    page.getByText("Доступен через службу", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText("1.12.25 · Проверено")).toBeVisible();
  await refresh.click();
  await expect(page.getByRole("status", { name: "Состояние службы" })).toContainText("Служба недоступна");
  await expect(page.getByText("Ядро службы: sing-box")).toHaveCount(0);
  await expect(page.getByText("Не подтверждён", { exact: true })).toBeVisible();
});

for (const delayedHealth of [false, true]) {
  test(`service loss clears positive diagnostics${delayedHealth ? " and rejects late health" : ""}`, async ({
    page,
  }) => {
    await page.addInitScript((delayed: boolean) => {
      const w = window as unknown as {
        isTauri: boolean;
        __TAURI_INTERNALS__: unknown;
        loseService: () => void;
        releaseOldHealth: () => void;
        pendingHealth: boolean;
        oldHealthReturned: boolean;
      };
      w.isTauri = true;
      w.pendingHealth = false;
      w.oldHealthReturned = false;
      let unavailable = false;
      let checks = 0;
      let release: (() => void) | undefined;
      w.loseService = () => {
        unavailable = true;
      };
      w.releaseOldHealth = () => {
        release?.();
      };
      w.__TAURI_INTERNALS__ = {
        invoke: async (cmd: string) => {
          if (cmd === "tunnel_status") {
            if (unavailable) throw new Error("Служба недоступна");
            return { phase: "disconnected", ownedByCaller: false, error: null };
          }
          if (cmd === "service_status") {
            if (++checks > 1 && delayed) {
              w.pendingHealth = true;
              await new Promise<void>((resolve) => {
                release = resolve;
              });
              w.oldHealthReturned = true;
            }
            return {
              version: "0.3.1",
              protocol: 2,
              systemTunnel: true,
              engines: [
                { name: "xray", version: "26.10.21", available: true },
                { name: "sing-box", version: "1.12.25", available: true },
              ],
            };
          }
          return {
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
        },
      };
    }, delayedHealth);
    await page.goto("/");
    await page.getByRole("button", { name: "Настройки", exact: true }).click();
    const refresh = page.getByRole("button", {
      name: "Проверить связь со службой",
      exact: true,
    });
    await refresh.click();
    await expect(page.getByRole("status", { name: "Состояние службы" })).toContainText(
      "Связь установлена · 0.3.1",
    );
    await expect(page.getByText("Ядро службы: sing-box")).toBeVisible();
    if (delayedHealth) {
      await refresh.click();
      await page.waitForFunction(
        () => (window as unknown as { pendingHealth: boolean }).pendingHealth,
      );
    }
    await page.evaluate(() =>
      (window as unknown as { loseService: () => void }).loseService(),
    );
    await expect(page.getByRole("status", { name: "Состояние службы" })).toHaveText("Служба недоступна");
    await expect(
      page.getByText("Связь установлена", { exact: false }),
    ).toHaveCount(0);
    await expect(page.getByText("Ядро службы:", { exact: false })).toHaveCount(
      0,
    );
    await expect(page.getByText("Совместим · IPC 2")).toHaveCount(0);
    await expect(
      page.getByText("Доступен через службу", { exact: true }),
    ).toHaveCount(0);
    if (delayedHealth) {
      await page.evaluate(() =>
        (
          window as unknown as { releaseOldHealth: () => void }
        ).releaseOldHealth(),
      );
      await page.waitForFunction(
        () =>
          (window as unknown as { oldHealthReturned: boolean })
            .oldHealthReturned,
      );
      await expect(page.getByRole("status", { name: "Состояние службы" })).toHaveText("Служба недоступна");
      await expect(
        page.getByText("Связь установлена", { exact: false }),
      ).toHaveCount(0);
      await expect(
        page.getByText("Ядро службы:", { exact: false }),
      ).toHaveCount(0);
      await expect(page.getByText("Совместим · IPC 2")).toHaveCount(0);
      await expect(
        page.getByText("Доступен через службу", { exact: true }),
      ).toHaveCount(0);
      await expect(refresh).toBeEnabled();
    }
  });
}
