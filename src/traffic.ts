import {invoke} from "@tauri-apps/api/core";
import {getCurrentWindow} from "@tauri-apps/api/window";
import {native} from "./api";
import {currentLanguage} from "./preferences";
type Connection={network:string;destinationIp:string|null;host:string|null;port:number|null;process:string|null;uploadedBytes:number;downloadedBytes:number;via:string};
type Traffic={session:string;elapsedSeconds:number;uploadedBytes:number;downloadedBytes:number;uploadPerSecond:number|null;downloadPerSecond:number|null;connectionsTotal:number;connections:Connection[]};
type History={enabled:boolean;entries:{firstSeen:number;lastSeen:number;uploadedBytes:number;downloadedBytes:number}[]};
const t=(ru:string,en:string)=>currentLanguage()==="en"?en:ru;
const e=(s:string)=>s.replace(/[&<>"']/g,c=>({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"})[c]!);
const bytes=(value:number|null)=>{if(value===null)return "—";let unit=0;const units=["B","KiB","MiB","GiB","TiB"];while(value>=1024&&unit<units.length-1){value/=1024;unit++;}return `${value.toLocaleString(undefined,{maximumFractionDigits:unit?2:0})} ${units[unit]}`;};
let latest:Traffic|null=null;
let available=false;
let history:History={enabled:false,entries:[]};
let historyAt=0;
let root:HTMLElement|null=null;
let pending=false;
let windowFocused=true;
let nextAt=0;
let points:{at:number;up:number;down:number}[]=[];
function chart():string {
  if(points.length<2)return `<p class="muted">${t("График появится после двух измерений скорости.","The chart appears after two speed samples.")}</p>`;
  const from=points[0].at,to=points[points.length-1].at,max=Math.max(1,...points.flatMap(p=>[p.up,p.down]));
  const line=(key:"up"|"down")=>points.map(p=>`${10+580*(p.at-from)/Math.max(1,to-from)},${110-100*p[key]/max}`).join(" ");
  return `<svg class="traffic-chart" viewBox="0 0 600 125" role="img" aria-label="${t("Измеренные скорости загрузки и отдачи","Measured download and upload speeds")}"><path d="M10 10v100h580" fill="none" stroke="currentColor" opacity=".3"/><polyline points="${line("down")}" fill="none" stroke="#77ad85" stroke-width="2"/><polyline points="${line("up")}" fill="none" stroke="#809ed2" stroke-width="2"/></svg><p class="muted">${t("Зелёный: загрузка · Синий: отдача · Последние 60 измерений","Green: download · Blue: upload · Last 60 samples")} · max ${bytes(max)}/s</p>`;
}
function live():string {
  if(!available||!latest)return `<p role="status">${t("Нет доступных счётчиков вашей активной сессии. Нужна служба 0.3.5 и поддерживаемое ядро.","No counters are available for your active session. Service 0.3.5 and a supported engine are required.")}</p>`;
  const s=latest;
  const seconds=s.elapsedSeconds;
  const elapsed=`${Math.floor(seconds/3600)}:${String(Math.floor(seconds%3600/60)).padStart(2,"0")}:${String(seconds%60).padStart(2,"0")}`;
  return `<div class="traffic-metrics"><div><span>${t("Загрузка","Download")}</span><strong>${bytes(s.downloadPerSecond)}/s</strong></div><div><span>${t("Отдача","Upload")}</span><strong>${bytes(s.uploadPerSecond)}/s</strong></div><div><span>${t("Получено","Received")}</span><strong>${bytes(s.downloadedBytes)}</strong></div><div><span>${t("Отправлено","Sent")}</span><strong>${bytes(s.uploadedBytes)}</strong></div><div><span>${t("Время сессии ядра","Engine session time")}</span><strong>${elapsed}</strong></div></div>${chart()}
  <h2>${t("Активные соединения","Active connections")} · ${s.connectionsTotal}</h2><p class="muted">${t("Показаны до 128 соединений. Имена процессов и адреса отображаются только при наличии данных ядра; в историю они не сохраняются.","Up to 128 connections are shown. Process names and destinations appear only when supplied by the engine; they are not saved in history.")}</p>
  <div class="data-table"><table><thead><tr>${[t("Процесс","Process"),t("Назначение","Destination"),t("Сеть","Network"),t("Маршрут","Route"),"↑","↓"].map(x=>`<th scope="col">${x}</th>`).join("")}</tr></thead><tbody>${s.connections.map(c=>`<tr><td translate="no">${e(c.process||"—")}</td><td translate="no">${e(c.host||c.destinationIp||"—")}${c.port===null?"":`:${c.port}`}</td><td>${e(c.network)}</td><td>${e(c.via)}</td><td>${bytes(c.uploadedBytes)}</td><td>${bytes(c.downloadedBytes)}</td></tr>`).join("")||`<tr><td colspan="6">${t("Нет активных соединений","No active connections")}</td></tr>`}</tbody></table></div>`;
}
function historyRows():string {
  return `<div class="data-table"><table><thead><tr><th>${t("Последнее измерение","Last observation")}</th><th>${t("Отправлено","Sent")}</th><th>${t("Получено","Received")}</th></tr></thead><tbody>${history.entries.slice().reverse().map(row=>`<tr><td>${e(new Date(row.lastSeen*1000).toLocaleString())}</td><td>${bytes(row.uploadedBytes)}</td><td>${bytes(row.downloadedBytes)}</td></tr>`).join("")||`<tr><td colspan="3">${t("Сохранённых измерений нет","No saved observations")}</td></tr>`}</tbody></table></div>`;
}
export function trafficHtml():string {
  return `<div class="page-heading"><div><p class="eyebrow">${t("ИЗМЕРЕНИЯ","MEASUREMENTS")}</p><h1>${t("Статистика и соединения","Traffic and connections")}</h1><p class="muted">${t("Реальные счётчики ядра. Данные обновляются, пока это окно активно.","Actual engine counters. Data updates while this window is active.")}</p></div></div>
  <article class="settings-card" data-traffic-view>${live()}</article><article class="settings-card"><h2>${t("История расхода","Traffic history")}</h2>
  <label><input type="checkbox" data-history-enable ${history.enabled?"checked":""}> ${t("Сохранять измерения расхода","Save traffic observations")}</label>
  <p class="muted">${t("По умолчанию выключено. Только время и объёмы, без адресов и процессов; защищённое хранилище, максимум 128 сессий за 90 дней. Сохранение — не чаще раза в 30 секунд. При аварийном завершении последние несохранённые измерения могут потеряться. История не переносится в backup.","Off by default. Only times and byte counts, without destinations or processes; protected storage, up to 128 sessions over 90 days. Saved at most once every 30 seconds. Unsaved observations can be lost on a crash. History is excluded from backups.")}</p>
  <div data-history-rows>${historyRows()}</div><button class="button subtle" data-history-clear>${t("Очистить историю…","Clear history…")}</button><p role="alert" data-history-error></p>
  <dialog id="history-clear"><h2>${t("Удалить сохранённые измерения?","Delete saved observations?")}</h2><p>${t("Новые записи начнутся после следующего измерения. Подключение и профили не меняются.","New records start after the next observation. The connection and profiles are unchanged.")}</p><div class="dialog-actions"><button class="button subtle" data-history-cancel>${t("Отмена","Cancel")}</button><button class="button primary" data-history-confirm>${t("Очистить","Clear")}</button></div></dialog></article>`;
}
export function bindTraffic(element:HTMLElement):void {
  root=element;
  const check=element.querySelector<HTMLInputElement>("[data-history-enable]");if(!check)return;
  const dialog=element.querySelector<HTMLDialogElement>("#history-clear")!;
  const change=async(clear:boolean)=>{
    try {history=await invoke<History>("set_traffic_history",{enabled:check.checked,clear});historyAt=Date.now();element.querySelector("[data-history-rows]")!.innerHTML=historyRows();dialog.close();}
    catch {element.querySelector("[data-history-error]")!.textContent=t("Не удалось изменить историю в защищённом хранилище","Could not change history in protected storage");}
  };
  check.onchange=()=>void change(false);
  element.querySelector<HTMLButtonElement>("[data-history-clear]")!.onclick=()=>dialog.showModal();
  element.querySelector<HTMLButtonElement>("[data-history-cancel]")!.onclick=()=>dialog.close();
  element.querySelector<HTMLButtonElement>("[data-history-confirm]")!.onclick=()=>void change(true);
  nextAt=0;historyAt=0;
}
async function poll():Promise<void> {
  if(!native||!root?.querySelector("[data-traffic-view]")||document.visibilityState!=="visible"||!windowFocused||pending||Date.now()<nextAt)return;
  pending=true;nextAt=Date.now()+1000;
  try {
    const next=await invoke<Traffic>("traffic_snapshot");
    if(!next||!Array.isArray(next.connections)||typeof next.uploadedBytes!=="number")throw new Error();
    if(latest?.session!==next.session||(points.length&&Date.now()-points[points.length-1].at>5000))points=[];
    latest=next;available=true;
    if(next.downloadPerSecond!==null&&next.uploadPerSecond!==null){points.push({at:Date.now(),up:next.uploadPerSecond,down:next.downloadPerSecond});points=points.slice(-60);}
  } catch {available=false;}
  try {
    if(Date.now()-historyAt>30000){const next=await invoke<History>("traffic_history");if(Array.isArray(next?.entries)){history=next;historyAt=Date.now();const check=root?.querySelector<HTMLInputElement>("[data-history-enable]");if(check)check.checked=history.enabled;const rows=root?.querySelector("[data-history-rows]");if(rows)rows.innerHTML=historyRows();}}
  } catch { /* Live measurements remain usable if protected history is unavailable. */ }
  const view=root?.querySelector("[data-traffic-view]");if(view)view.innerHTML=live();
  pending=false;
}
setInterval(()=>void poll(),500);
if(native){try{void getCurrentWindow().onFocusChanged(event=>{windowFocused=event.payload;if(windowFocused)nextAt=0;}).catch(()=>{});}catch{/* Browser-only adapters have no native window metadata. */}}
