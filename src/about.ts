import {invoke} from "@tauri-apps/api/core";
import {type Snapshot,native} from "./api";
import {currentLanguage} from "./preferences";
const t=(ru:string,en:string)=>currentLanguage()==="en"?en:ru;
const e=(s:string)=>s.replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"})[c]!);
export function aboutHtml(state:Snapshot):string {
  return `<div class="page-heading"><div><p class="eyebrow">MOTH DESKTOP</p><h1>${t("О приложении","About")}</h1></div></div><article class="settings-card"><h2>Moth ${e(state.version)}</h2><p>${t("Настольный клиент на закреплённых Xray и sing-box.","Desktop client using pinned Xray and sing-box engines.")}</p><p>GPL-3.0-or-later · ${e(state.capabilities.platform)}</p>
  <div class="setting"><span>${t("Защищённое хранилище","Protected storage")}</span><strong>${state.capabilities.secureStorage?(state.capabilities.platform==="linux"?"Secret Service":"Windows DPAPI"):t("Недоступно в этом режиме","Unavailable in this mode")}</strong></div>
  <p class="muted">${t("В установленном приложении профили и ссылки защищены DPAPI в Windows и Secret Service в Ubuntu. Адреса соединений не сохраняются в истории расхода. Файлы лицензий и сведения о зависимостях входят в установочный пакет.","In the installed application, profiles and URLs are protected by Windows DPAPI and Ubuntu Secret Service. Connection destinations are not stored in traffic history. License files and dependency notices ship with the package.")}</p>
  <p translate="no">https://github.com/Bu11t0N/Moth-Desktop</p>
  <div class="library-buttons"><button class="button subtle" data-license="project">GPL-3.0</button><button class="button subtle" data-license="rust">${t("Лицензии Rust","Rust licenses")}</button><button class="button subtle" data-license="javaScript">Tauri API · MIT</button><button class="button subtle" data-license="thirdParty">${t("Сторонние компоненты","Third-party components")}</button></div>
  <p role="alert" data-about-error></p></article><article class="settings-card"><h2>${t("Закреплённые ядра","Pinned engines")}</h2>${state.engines.map(engine=>`<div class="setting"><span>${e(engine.name)} ${e(engine.version)}</span><strong>${engine.available?t("Хэш проверен","Hash verified"):t("Недоступно","Unavailable")}</strong></div>`).join("")}</article>`;
}
export function bindAbout(root:HTMLElement):void {
  root.querySelectorAll<HTMLButtonElement>("[data-license]").forEach(button=>button.onclick=async()=>{
    try {
      if(!native)throw new Error();
      const doc=await invoke<{name:string;mime:string;text:string}>("license_document",{kind:button.dataset.license});
      const url=URL.createObjectURL(new Blob([doc.text],{type:doc.mime}));const link=document.createElement("a");link.href=url;link.download=doc.name;link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);
    } catch {root.querySelector("[data-about-error]")!.textContent=t("Документы доступны в установленном приложении","Documents are available in the installed application");}
  });
}
