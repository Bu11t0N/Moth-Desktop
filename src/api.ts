import { invoke, isTauri } from "@tauri-apps/api/core";

export type Profile = {
  id: string;
  name: string;
  host: string;
  port: number;
  protocol: string;
  transport: string;
  security: string;
};
export type Snapshot = {
  profiles: Profile[];
  selected: string | null;
  version: string;
  capabilities: {
    platform: string;
    secureStorage: boolean;
    systemTunnel: boolean;
    killSwitch: boolean;
  };
};
export const native = isTauri();
export const empty: Snapshot = {
  profiles: [],
  selected: null,
  version: "0.1.0",
  capabilities: {
    platform: "browser",
    secureStorage: false,
    systemTunnel: false,
    killSwitch: false,
  },
};
export async function call(
  command: string,
  args?: Record<string, unknown>,
): Promise<Snapshot> {
  if (!native) {
    if (command === "get_snapshot") return structuredClone(empty);
    throw new Error(
      "Импорт доступен в приложении Windows. В браузере открыт только просмотр интерфейса.",
    );
  }
  return invoke<Snapshot>(command, args);
}
