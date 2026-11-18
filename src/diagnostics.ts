import {invoke} from "@tauri-apps/api/core";
import {native} from "./api";
import {currentLanguage} from "./preferences";
type Event={at:number;level:string;category:string;code:string;reason:string|null};
type Snapshot={version:string;platform:string;events:Event[]};
const t=(ru:string,en:string)=>currentLanguage()==="en"?en:ru;
const e=(s:string)=>s.replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"})[c]!);
const messages:Record<string,[string,string]>={
  autostartEnabled:["Запись автозапуска включена","Startup registration enabled"],autostartDisabled:["Собственная запись автозапуска отключена","Owned startup registration disabled"],autostartFailed:["Автозапуск не изменён или не подтверждён","Startup registration change failed or is unverified"],
  disconnectStarted:["Начато отключение","Disconnection started"],
  connectStarted:["Начато подключение","Connection started"],connectVerified:["Служба подтвердила канал подключения","Service verified the connection channel"],connectFailed:["Подключение или проверка туннеля не подтверждены","Connection or tunnel verification failed"],disconnected:["Соединение отключено","Connection disconnected"],recovering:["Служба восстанавливает подключение","Service is recovering the connection"],serviceUnavailable:["Связь со службой недоступна; защита не подтверждена","Service is unavailable; protection is unverified"],
  subscriptionStarted:["Начато обновление подписки","Subscription refresh started"],subscriptionSucceeded:["Список подписки сохранён","Subscription list saved"],subscriptionFailed:["Подписка отклонена; подробности в карточке источника","Subscription refresh rejected; see its source card"],
  updateCheck:["Проверяются подписанные сведения обновления","Checking signed update metadata"],updateDownload:["Загружается пакет обновления","Downloading update package"],updateInstall:["Начата установка обновления","Update installation started"],updateSuccess:["Установка обновления завершена","Update installation completed"],updateFailed:["Обновление прервано; проверьте состояние установки","Update failed; inspect installation status"],storageFailed:["Защищённое хранилище отклонило сохранение","Protected storage rejected a save"],
  guardEnable:["Запрошено включение kill switch","Kill switch activation requested"],guardRelease:["Запрошено снятие блокировки Moth","Moth network block release requested"],guardVerified:["Изменение правил Moth подтверждено","Moth firewall change verified"],guardFailed:["Изменение правил не подтверждено; блокировка может сохраняться","Firewall change unverified; a block may remain"],
};
let snapshot:Snapshot|null=null;
let preview="";
let level="all";
let pending=false;
function rows():string {
  const events=(snapshot?.events||[]).filter(row=>level==="all"||row.level===level);
  return `<div class="data-table"><table><thead><tr><th>${t("Время","Time")}</th><th>${t("Уровень","Level")}</th><th>${t("Стадия","Stage")}</th><th>${t("Событие","Event")}</th></tr></thead><tbody>${events.slice().reverse().map(row=>`<tr><td>${e(new Date(row.at*1000).toLocaleTimeString())}</td><td>${e(row.level)}</td><td>${e(row.category)}</td><td>${e(messages[row.code]?t(...messages[row.code]):t("Неизвестный код события","Unknown event code"))}${row.reason?` · <code>${e(row.reason)}</code>`:""}</td></tr>`).join("")||`<tr><td colspan="4">${t("В этом сеансе событий нет","No events in this application session")}</td></tr>`}</tbody></table></div>`;
}
export function diagnosticsHtml():string {
  return `<div class="page-heading"><div><p class="eyebrow">${t("СОСТОЯНИЕ","STATUS")}</p><h1>${t("Диагностика","Diagnostics")}</h1><p class="muted">${t("Стадии подключения, подписок, хранения и обновления.","Connection, subscription, storage and update stages.")}</p></div></div><article class="settings-card"><h2>${t("Журнал событий","Event journal")}</h2>
  <p class="muted">${t("До 256 событий в памяти текущего приложения. URL подписок, токены, UUID профилей, ключи, HWID, адреса соединений и персональные пути не записываются. После выхода журнал очищается. Экспорт содержит только показанные поля, версию и платформу.","Up to 256 events in this application process. Subscription URLs, tokens, profile UUIDs, keys, HWID, connection destinations and personal paths are not recorded. The journal clears on exit. Export contains only these fields, version and platform.")}</p>
  <div class="library-toolbar"><label>${t("Уровень","Level")}<select data-diagnostic-level>${["all","info","warning","error"].map(v=>`<option ${level===v?"selected":""} value="${v}">${v}</option>`).join("")}</select></label><button class="button subtle" data-diagnostic-refresh>${t("Обновить","Refresh")}</button><button class="button subtle" data-diagnostic-clear>${t("Очистить журнал","Clear journal")}</button><button class="button primary" data-diagnostic-preview>${t("Предпросмотр экспорта","Preview export")}</button></div>
  <div data-diagnostic-rows>${rows()}</div><p role="alert" data-diagnostic-error></p>
  <dialog id="diagnostic-preview" aria-labelledby="diagnostic-title"><h2 id="diagnostic-title">${t("Проверьте содержимое перед экспортом","Review content before export")}</h2><pre class="diagnostic-preview" data-diagnostic-text translate="no"></pre><div class="dialog-actions"><button class="button subtle" data-diagnostic-close>${t("Закрыть","Close")}</button><button class="button primary" data-diagnostic-download>${t("Скачать JSON","Download JSON")}</button></div></dialog></article>`;
}
export function bindDiagnostics(root:HTMLElement):void {
  const container=root.querySelector("[data-diagnostic-rows]");if(!container)return;
  const error=root.querySelector("[data-diagnostic-error]")!;
  const refresh=async(clear=false)=>{
    if(pending)return;pending=true;
    try {
      if(!native)throw new Error();
      const next=await invoke<Snapshot>(clear?"clear_diagnostics":"diagnostic_snapshot");
      if(!Array.isArray(next?.events))throw new Error();
      snapshot=next;container.innerHTML=rows();error.textContent="";
    } catch {error.textContent=t("Журнал доступен в настольном приложении","The journal is available in the desktop application");}
    finally{pending=false;}
  };
  root.querySelector<HTMLSelectElement>("[data-diagnostic-level]")!.onchange=event=>{level=(event.target as HTMLSelectElement).value;container.innerHTML=rows();};
  root.querySelector<HTMLButtonElement>("[data-diagnostic-refresh]")!.onclick=()=>void refresh();
  root.querySelector<HTMLButtonElement>("[data-diagnostic-clear]")!.onclick=()=>void refresh(true);
  const dialog=root.querySelector<HTMLDialogElement>("#diagnostic-preview")!;
  root.querySelector<HTMLButtonElement>("[data-diagnostic-preview]")!.onclick=async()=>{
    await refresh();if(!snapshot)return;
    preview=JSON.stringify(snapshot,null,2);root.querySelector("[data-diagnostic-text]")!.textContent=preview;dialog.showModal();
  };
  root.querySelector<HTMLButtonElement>("[data-diagnostic-close]")!.onclick=()=>{dialog.close();preview="";root.querySelector("[data-diagnostic-text]")!.textContent="";};
  dialog.onclose=()=>{preview="";root.querySelector("[data-diagnostic-text]")!.textContent="";};
  root.querySelector<HTMLButtonElement>("[data-diagnostic-download]")!.onclick=()=>{
    if(!preview)return;
    const url=URL.createObjectURL(new Blob([preview],{type:"application/json"}));const link=document.createElement("a");link.href=url;link.download="Moth-diagnostics.json";link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
  };
  void refresh();
}
