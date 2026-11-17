import {invoke} from "@tauri-apps/api/core";
import {native} from "./api";
import {currentLanguage} from "./preferences";
type GuardState={enabled:boolean;filtering:boolean;ownedByCaller:boolean};
let state:GuardState|null=null;
let verified=false;
const t=(ru:string,en:string)=>currentLanguage()==="en"?en:ru;
export const guardEnabled=()=>state?.enabled===true;
export const guardVerified=()=>verified&&state?.filtering===true;
const summary=()=>!verified?t("Состояние kill switch не подтверждено. Блокировка может сохраняться.","Kill switch status is unverified. A network block may remain."):
  state?.enabled?(state.filtering?t("Kill switch включён. Disconnect и Exit сохраняют блокировку.","Kill switch is on. Disconnect and Exit keep the network blocked."):t("Kill switch выбран, но правила ОС не подтверждены. Защита не гарантирована.","Kill switch is selected but OS rules are unverified. Protection is not guaranteed.")):
  t("Kill switch выключен. При сбое туннеля обычная сеть может восстановиться.","Kill switch is off. Normal networking can return after tunnel failure.");
export function guardBanner():string {
  if(!state?.enabled)return "";
  return `<div class="preview-note" role="status">${summary()}</div>`;
}
export function guardHtml():string {
  return `<article class="settings-card"><h2>Persistent kill switch</h2><p role="status">${summary()}</p>
  <p class="muted">${t("Блокируется прямой IPv4/IPv6/DNS для всего компьютера. При сбое ядер, службы или окна, ручном отключении и выходе блокировка остаётся. Исключения ограничены loopback, необходимыми сетевыми служебными пакетами и закреплёнными адресами VPN. Direct-правила и разделение приложений несовместимы с этой настройкой.","Direct IPv4/IPv6/DNS is blocked for the whole computer. The block remains after engine, service or window failure, manual disconnect and exit. Exceptions are limited to loopback, essential network control traffic and pinned VPN endpoints. Direct rules and application splitting are incompatible with this setting.")}</p>
  <button class="button subtle" data-guard-refresh>${t("Проверить kill switch","Check kill switch")}</button>
  <button class="button ${state?.enabled?"subtle":"primary"}" data-guard-change ${!verified||(state?.enabled&&!state.ownedByCaller)?"disabled":""}>${state?.enabled?t("Снять блокировку Moth","Release Moth network block"):t("Включить kill switch…","Enable kill switch…")}</button>
  <p class="muted">${t("Сначала установите полный пакет 0.3.5 и подключите VPN. Чтобы сменить сервер на ещё не закреплённый адрес, может потребоваться явно снять блокировку. Подписки и обновления не получают прямого обходного канала.","Install the complete 0.3.5 package and connect the VPN first. Switching to an endpoint that was not pinned may require explicitly releasing the block. Subscriptions and updates receive no direct bypass.")}</p>
  <details><summary>${t("Восстановление сети администратором","Administrator network recovery")}</summary><p>${t("Если служба недоступна: остановите службу Moth и выполните moth-service --release-guard от администратора; в Ubuntu используйте sudo /usr/lib/moth-desktop/moth-service --release-guard. Удаляются только правила Moth. Обычный restart сохраняет блокировку; штатное удаление приложения снимает её.","If the service is unavailable, stop the Moth service and run moth-service --release-guard as administrator; on Ubuntu use sudo /usr/lib/moth-desktop/moth-service --release-guard. Only Moth rules are removed. An ordinary restart retains the block; normal application uninstall releases it.")}</p></details>
  <p role="alert" data-guard-error></p>
  <dialog id="guard-confirm" aria-labelledby="guard-confirm-title"><h2 id="guard-confirm-title">${state?.enabled?t("Разрешить обычную сеть?","Allow normal networking?"):t("Оставлять интернет заблокированным без VPN?","Keep networking blocked without VPN?")}</h2>
  <p>${state?.enabled?t("Защита Moth от прямого выхода будет снята. Активный VPN продолжит работу, но при его сбое сеть сможет вернуться к обычному подключению.","Moth's direct egress protection will be removed. The active VPN will continue, but a failure can return traffic to normal networking."):t("Это настройка всего компьютера. После Disconnect, Exit и перезапуска службы сеть останется заблокированной, пока вы не подключите VPN заново или явно не снимете блокировку. Не включайте с direct-правилами или разделением приложений.","This is a computer-wide setting. After Disconnect, Exit and service restart, networking remains blocked until you reconnect the VPN or explicitly release the block. Do not enable it with direct rules or application splitting.")}</p>
  <p role="alert" data-guard-dialog-error></p><div class="dialog-actions"><button class="button subtle" data-guard-cancel>${t("Отмена","Cancel")}</button><button class="button primary" data-guard-confirm>${state?.enabled?t("Снять блокировку","Release block"):t("Понимаю, включить","I understand, enable")}</button></div></dialog></article>`;
}
export async function refreshGuard():Promise<boolean> {
  const before=JSON.stringify([state,verified]);
  if(!native)return false;
  try {
    const next=await invoke<GuardState>("egress_guard_status");
    if(typeof next?.enabled!=="boolean"||typeof next?.filtering!=="boolean"||typeof next?.ownedByCaller!=="boolean")throw new Error();
    state=next;verified=true;
  } catch {verified=false;}
  return before!==JSON.stringify([state,verified]);
}
export function bindGuard(root:HTMLElement,render:()=>void):void {
  const dialog=root.querySelector<HTMLDialogElement>("#guard-confirm");if(!dialog)return;
  root.querySelector<HTMLButtonElement>("[data-guard-refresh]")!.onclick=()=>void refreshGuard().then(()=>render());
  root.querySelector<HTMLButtonElement>("[data-guard-change]")!.onclick=()=>dialog.showModal();
  const cancel=root.querySelector<HTMLButtonElement>("[data-guard-cancel]")!;
  cancel.onclick=()=>dialog.close();
  const confirm=root.querySelector<HTMLButtonElement>("[data-guard-confirm]")!;
  confirm.onclick=async()=>{
    const enabled=!state?.enabled;
    confirm.disabled=true;cancel.disabled=true;
    dialog.oncancel=e=>e.preventDefault();
    try {
      state=await invoke<GuardState>("set_egress_guard",{enabled});verified=true;
      dialog.close();render();
    } catch(e) {
      verified=false;
      root.querySelector("[data-guard-dialog-error]")!.textContent=currentLanguage()==="en"?"Could not confirm the firewall change. The network block may remain. Check service status or use administrator recovery.":String(e);
    } finally {confirm.disabled=false;cancel.disabled=false;dialog.oncancel=null;}
  };
}
