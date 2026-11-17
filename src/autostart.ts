import {invoke} from "@tauri-apps/api/core";
import {native} from "./api";
import {currentLanguage} from "./preferences";
type Status={supported:boolean;registered:boolean;owned:boolean;currentTarget:boolean;conflict:boolean};
const t=(ru:string,en:string)=>currentLanguage()==="en"?en:ru;
export function autostartHtml():string {
  return `<article class="settings-card"><h2>${t("Запуск при входе","Start at login")}</h2><label><input type="checkbox" data-autostart disabled> ${t("Запускать Moth при входе в учётную запись","Start Moth when signing in")}</label>
  <p class="muted">${t("GUI запускается от обычного пользователя. Подключение последнего сервера включается отдельно в сетевых настройках. Если сеть ещё недоступна, повторные попытки зависят от настройки автовосстановления; ручное отключение прекращает их.","The GUI starts as the ordinary user. Connecting to the last server is a separate network setting. If networking is unavailable, retries follow automatic recovery settings; manual disconnect stops them.")}</p>
  <p class="muted">${t("Moth управляет только собственной записью автозапуска. Внешние изменения и запреты ОС не перезаписываются. Эта настройка не переносится в backup; после обновления собственная запись может быть перенаправлена на текущую версию.","Moth manages only its own startup registration. External edits and OS restrictions are not overwritten. This setting is not transferred by backups; an owned registration can be retargeted to the current version after an update.")}</p>
  <p role="status" data-autostart-status></p><p role="alert" data-autostart-error></p><button class="button subtle" data-autostart-refresh>${t("Проверить запись автозапуска","Check startup registration")}</button></article>`;
}
export function bindAutostart(root:HTMLElement):void {
  const checkbox=root.querySelector<HTMLInputElement>("[data-autostart]");if(!checkbox)return;
  const status=root.querySelector("[data-autostart-status]")!;
  const error=root.querySelector("[data-autostart-error]")!;
  let known:Status|null=null;
  const show=(next:Status)=>{
    known=next;checkbox.checked=next.registered&&next.owned;checkbox.disabled=!next.supported||next.conflict;
    status.textContent=next.conflict?t("Запись изменена вне Moth и сохранена без изменений","The registration was changed outside Moth and is left untouched"):next.registered&&next.owned?(next.currentTarget?t("Собственная запись автозапуска зарегистрирована","Moth startup registration is present"):t("Запись указывает на другую версию; проверьте обновление приложения","The registration targets another version; check the application update")):t("Собственная запись автозапуска отсутствует","No Moth startup registration is present");
  };
  const refresh=async()=>{
    try {
      if(!native)throw new Error();
      const next=await invoke<Status>("autostart_status");if(typeof next?.supported!=="boolean")throw new Error();
      show(next);error.textContent="";
    }catch{checkbox.disabled=true;status.textContent=t("Не удалось проверить автозапуск в этом режиме","Startup registration could not be checked in this mode");}
  };
  checkbox.onchange=async()=>{
    checkbox.disabled=true;
    try{const next=await invoke<Status>("set_autostart",{enabled:checkbox.checked});show(next);error.textContent="";}
    catch(e){checkbox.checked=!!known?.registered&&!!known?.owned;error.textContent=currentLanguage()==="en"?"Could not change startup registration. External entries are preserved.":String(e);await refresh();}
  };
  root.querySelector<HTMLButtonElement>("[data-autostart-refresh]")!.onclick=()=>void refresh();
  void refresh();
}
