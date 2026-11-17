import {test,expect} from "@playwright/test";
test.beforeEach(async({page})=>{
  await page.addInitScript(()=>{
    const w=window as unknown as {isTauri:boolean;__TAURI_INTERNALS__:unknown;trafficCalls:number};w.isTauri=true;w.trafficCalls=0;
    let history=false;let events=[{at:1700000000,level:"error",category:"connection",code:"connectFailed",reason:"probeFailed"}];
    const state={profiles:[],selected:null,version:"0.3.5",engines:[],privateFixture:"synthetic-hwid-do-not-export",capabilities:{platform:"windows",secureStorage:true,systemTunnel:true,killSwitch:true}};
    w.__TAURI_INTERNALS__={invoke:async(command:string,args?:{enabled:boolean})=>{
      if(command==="update_monitor_mode")return false;
      if(command==="tunnel_status")return{phase:"connected",ownedByCaller:true,error:null};
      if(command==="egress_guard_status")return{enabled:false,filtering:false,ownedByCaller:false};
      if(command==="traffic_snapshot"){w.trafficCalls++;return{session:"00000000000000000000000000000001",elapsedSeconds:60,uploadedBytes:1024,downloadedBytes:2048,uploadPerSecond:null,downloadPerSecond:null,connectionsTotal:1,connections:[{network:"tcp",destinationIp:"192.0.2.1",host:"example.com",port:443,process:"<img src=x onerror=alert(1)>",uploadedBytes:100,downloadedBytes:200,via:"proxy"}]};}
      if(command==="set_traffic_history")history=args!.enabled;
      if(command==="traffic_history"||command==="set_traffic_history")return{enabled:history,entries:[]};
      if(command==="clear_diagnostics")events=[];
      if(command==="diagnostic_snapshot"||command==="clear_diagnostics")return{version:"0.3.5",platform:"windows",events};
      return state;
    }};
  });
});
test("traffic uses provided counters, escapes connections and pauses while hidden",async({page})=>{
  await page.goto("/");await page.locator('[data-page="activity"]').click();
  const view=page.locator("[data-traffic-view]");
  await expect(view).toContainText("2 KiB");await expect(view).toContainText("0:01:00");
  await expect(view.locator("img")).toHaveCount(0);await expect(view).toContainText("<img src=x onerror=alert(1)>");
  await expect(page.locator("[data-history-enable]")).not.toBeChecked();
  await page.evaluate(()=>Object.defineProperty(document,"visibilityState",{value:"hidden",configurable:true}));
  const before=await page.evaluate(()=>(window as unknown as {trafficCalls:number}).trafficCalls);
  await page.waitForTimeout(1600);
  const after=await page.evaluate(()=>(window as unknown as {trafficCalls:number}).trafficCalls);
  expect(after).toBeLessThanOrEqual(before+1);
});
test("diagnostic export is previewed and excludes unrelated application state",async({page})=>{
  await page.goto("/");await page.locator('[data-page="diagnostics"]').click();
  await expect(page.locator("[data-diagnostic-rows]")).toContainText("probeFailed");
  await page.getByRole("button",{name:"Предпросмотр экспорта",exact:true}).click();
  const preview=page.locator("#diagnostic-preview");await expect(preview).toBeVisible();
  const body=await preview.locator("pre").innerText();
  expect(JSON.parse(body).events[0].code).toBe("connectFailed");expect(body).not.toContain("synthetic-hwid-do-not-export");
  await preview.getByRole("button",{name:"Закрыть",exact:true}).click();
  await page.getByRole("button",{name:"Очистить журнал",exact:true}).click();
  await expect(page.locator("[data-diagnostic-rows]")).not.toContainText("probeFailed");
});
