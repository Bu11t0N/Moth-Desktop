import {invoke} from "@tauri-apps/api/core";
import {native,type Snapshot,type ProxyPreferences} from "./api";
import {currentLanguage} from "./preferences";
const t=(ru:string,en:string)=>currentLanguage()==="en"?en:ru;
const escape=(s:string)=>s.replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"})[c]!);
export function proxyHtml(state:Snapshot):string {
  const p=state.proxyPreferences??{mode:"tun",shareWithTun:false,systemProxy:false,listener:{listen:"127.0.0.1",port:10808,allowedClients:[]}};
  return `<article class="settings-card"><h2>${t("Режим подключения и прокси","Connection mode and proxy")}</h2>
  <form data-proxy-form><label>${t("Режим следующего подключения","Next connection mode")} <select name="mode"><option value="tun" ${p.mode==="tun"?"selected":""}>${t("Системный VPN (TUN)","System VPN (TUN)")}</option><option value="proxy" ${p.mode==="proxy"?"selected":""}>${t("Только HTTP/SOCKS прокси","HTTP/SOCKS proxy only")}</option></select></label>
  <p class="muted">${t("Прокси обслуживает только настроенные на него приложения. TUN не создаётся; DNS, MTU, IPv6 и разделение приложений ниже относятся к TUN. Системный прокси зависит от поддержки в приложениях и исключений ОС.","Proxy mode serves only applications configured to use it. It creates no TUN; the DNS, MTU, IPv6 and per-app settings below apply to TUN. OS proxy use depends on application support and existing OS bypass rules.")}</p>
  <label><input type="checkbox" name="share" ${p.shareWithTun?"checked":""}> ${t("Также открыть HTTP/SOCKS при TUN","Also expose HTTP/SOCKS with TUN")}</label>
  <label><input type="checkbox" name="system" ${p.systemProxy?"checked":""}> ${t("Установить системный HTTP/HTTPS прокси для этой учётной записи","Set this user's OS HTTP/HTTPS proxy")}</label>
  <p class="muted">${t("Windows: пользовательские параметры WinINet; Ubuntu GNOME: GSettings. Программы могут игнорировать эти настройки. Браузер может запросить логин и пароль; откройте данные доступа после подключения.","Windows: per-user WinINet; Ubuntu GNOME: GSettings. Programs may ignore these settings. A browser may request a username and password; open access details after connecting.")}</p>
  <label>${t("Адрес привязки","Bind address")} <input name="address" value="${escape(p.listener.listen)}" maxlength="45" spellcheck="false" required></label>
  <label>${t("Порт HTTP/SOCKS","HTTP/SOCKS port")} <input type="number" name="port" min="1024" max="65535" value="${p.listener.port}" required></label>
  <label>${t("Разрешённые клиенты LAN (IP/CIDR, по одному в строке)","Allowed LAN clients (IP/CIDR, one per line)")}<textarea name="clients" rows="3" maxlength="1040" spellcheck="false">${escape(p.listener.allowedClients.join("\n"))}</textarea></label>
  <p class="muted">${t("По умолчанию 127.0.0.1. LAN требует точного частного адреса этого компьютера и списка клиентов; 0.0.0.0 и :: запрещены. Авторизация обязательна, пароль меняется при новом ручном подключении. LAN несовместим с kill switch. HTTP/SOCKS до этого компьютера не шифрует пароль: используйте только доверенную локальную сеть. Moth не открывает порты в других брандмауэрах.","The default is 127.0.0.1. LAN requires this computer's specific private address and a client allowlist; 0.0.0.0 and :: are rejected. Authentication is mandatory; a new manual session changes the password. LAN is incompatible with kill switch. The HTTP/SOCKS leg to this computer does not encrypt the password: use a trusted local network. Moth does not open ports in other firewalls.")}</p>
  <button class="button" ${!native?"disabled":""}>${t("Сохранить режим","Save mode")}</button><p data-proxy-message role="status"></p></form>
  <div class="actions"><button type="button" class="button subtle" data-proxy-access>${t("Показать данные доступа…","Show access details…")}</button><button type="button" class="button subtle" data-proxy-restore>${t("Отключить и восстановить системный прокси","Disconnect and restore OS proxy")}</button></div><p data-system-proxy-status role="status"></p>
  <p class="muted">${t("Disconnect, Exit и установка обновления восстанавливают только собственные изменения. После аварийного выхода откройте Moth и используйте восстановление. Если приложение недоступно: Windows → Параметры → Сеть и Интернет → Прокси; GNOME → Настройки → Сеть → Сетевой прокси. Проверьте прежние параметры вручную перед удалением приложения. Настройки режима и журнал восстановления не переносятся в backup.","Disconnect, Exit and update installation restore owned settings. After a crash, reopen Moth and use restore. If the app is unavailable: Windows → Settings → Network & Internet → Proxy; GNOME → Settings → Network → Network Proxy. Check your original settings before removing the app. Mode settings and recovery journals are not transferred by backups.")}</p></article>`;
}
export function bindProxy(root:HTMLElement,changed:(s:Snapshot)=>void):void {
  const form=root.querySelector<HTMLFormElement>("[data-proxy-form]");if(!form)return;
  const message=root.querySelector("[data-proxy-message]")!;
  const status=root.querySelector("[data-system-proxy-status]")!;
  const refresh=async()=>{try{if(!native)throw new Error();const s=await invoke<{supported:boolean;recoveryPending:boolean;owned:boolean}>("system_proxy_status");
    status.textContent=s.recoveryPending?(s.owned?t("Собственный системный прокси установлен; доступно восстановление","Moth OS proxy is set; restoration is available"):t("Есть журнал восстановления; внешние изменения будут сохранены","A recovery journal exists; external edits will be preserved")):t("Журнал собственных изменений отсутствует","No owned OS proxy changes are recorded");
  }catch{status.textContent=t("Системный прокси в этой среде недоступен или не подтверждён","OS proxy is unavailable or unverified in this environment");}};
  form.onsubmit=async(event)=>{event.preventDefault();const data=new FormData(form);
    const input:ProxyPreferences={mode:data.get("mode")==="proxy"?"proxy":"tun",shareWithTun:data.has("share"),systemProxy:data.has("system"),listener:{listen:String(data.get("address")).trim(),port:Number(data.get("port")),allowedClients:String(data.get("clients")).split(/\s+/).filter(Boolean)}};
    if(input.listener.listen!=="127.0.0.1"&&input.listener.listen!=="::1"&&!confirm(t("Разрешить авторизованный доступ из указанной доверенной LAN? После подключения данные доступа можно показать отдельно.","Allow authenticated access from the specified trusted LAN? Access details can be shown separately after connecting.")))return;
    if(input.systemProxy&&!confirm(t("При подключении Moth заменит пользовательский системный прокси. Disconnect и Exit восстанавливают собственные изменения; после сбоя доступно ручное восстановление. Продолжить?","On connection, Moth will replace your user OS proxy. Disconnect and Exit restore owned changes; manual restoration is available after a crash. Continue?")))return;
    try{changed(await invoke<Snapshot>("save_proxy_preferences",{input}));}catch(e){message.textContent=currentLanguage()==="en"?"Could not save proxy settings. Check the address, port, allowlist and listener choice.":String(e);}
  };
  root.querySelector<HTMLButtonElement>("[data-proxy-restore]")!.onclick=async()=>{
    try{changed(await invoke<Snapshot>("restore_system_proxy"));}catch(e){status.textContent=currentLanguage()==="en"?"Restoration could not be confirmed. Check the OS proxy settings; the recovery journal is retained.":String(e);}
  };
  root.querySelector<HTMLButtonElement>("[data-proxy-access]")!.onclick=async()=>{
    try{
      const access=await invoke<{address:string;port:number;username:string;password:string}>("proxy_access");
      const dialog=document.createElement("dialog");dialog.id="proxy-access";dialog.setAttribute("aria-label",t("Данные доступа к прокси","Proxy access details"));
      const title=document.createElement("h2");title.textContent=t("Данные доступа к HTTP/SOCKS","HTTP/SOCKS access details");dialog.append(title);
      for(const [label,value] of [[t("Адрес","Address"),access.address],[t("Порт","Port"),String(access.port)],[t("Логин","Username"),access.username],[t("Пароль","Password"),access.password]]){
        const row=document.createElement("label");row.textContent=label;const field=document.createElement("input");field.readOnly=true;field.value=value;field.autocomplete="off";field.spellcheck=false;field.setAttribute("translate","no");row.append(field);dialog.append(row);
      }
      const close=document.createElement("button");close.className="button";close.textContent=t("Закрыть","Close");close.onclick=()=>dialog.close();dialog.append(close);
      dialog.onclose=()=>{for(const field of dialog.querySelectorAll("input"))field.value="";access.password="";access.username="";dialog.remove();};root.append(dialog);dialog.showModal();
    }catch{message.textContent=t("Сначала подключите HTTP/SOCKS прокси своей учётной записью","First connect an HTTP/SOCKS proxy owned by your account");}
  };
  void refresh();
}
