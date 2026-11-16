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
  engines: {
    name: string;
    version: string;
    available: boolean;
    message: string | null;
  }[];
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
  version: "0.3.4",
  engines: [],
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
      "Импорт доступен в настольном приложении. В браузере открыт только просмотр интерфейса.",
    );
  }
  return invoke<Snapshot>(command, args);
}
export type ProbeResult = { profileId: string; latencyMs: number };
export async function probe(id: string): Promise<ProbeResult> {
  if (!native)
    throw new Error("Проверка сервера доступна в настольном приложении.");
  return invoke<ProbeResult>("probe_server", { id });
}
export type ServiceHealth = {
  version: string;
  protocol: number;
  systemTunnel: boolean;
  engines: { name: string; version: string; available: boolean }[];
};
export async function serviceStatus(): Promise<ServiceHealth> {
  if (!native)
    throw new Error("Проверка службы доступна в настольном приложении.");
  return invoke<ServiceHealth>("service_status");
}
export type TunnelState = {
  phase:
    "disconnected" | "connecting" | "connected" | "disconnecting" | "failed";
  ownedByCaller: boolean;
  error: string | null;
};
export async function tunnel(
  command: "tunnel_status" | "connect_vpn" | "disconnect_vpn",
  id?: string,
): Promise<TunnelState> {
  if (!native) throw new Error("VPN доступен в настольном приложении.");
  return invoke<TunnelState>(command, id ? { id } : undefined);
}
export async function exitApp(): Promise<void> {
  if (!native) throw new Error("Выход доступен в настольном приложении.");
  return invoke<void>("quit_app");
}
