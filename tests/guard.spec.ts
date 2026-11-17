import {test,expect} from "@playwright/test";
test("persistent guard requires confirmation and keeps blocking explicit at exit",async({page})=>{
  await page.addInitScript(()=>{
    const w=window as unknown as {isTauri:boolean;__TAURI_INTERNALS__:unknown;guardChanges:number};w.isTauri=true;w.guardChanges=0;
    let enabled=false;
    const settings={mtu:1500,ipv6:true,dns:"https://1.1.1.1/dns-query",bootstrapDns:"https://1.1.1.1/dns-query",rules:[],splitMode:"bypass",splitApps:[],healthEnabled:false,healthIntervalSeconds:300,healthUrl:"https://cp.cloudflare.com/generate_204",autoReconnect:false,failover:false,autoConnect:false,theme:"dark",language:"en"};
    const state={profiles:[],selected:null,version:"0.3.5",engines:[],settings,capabilities:{platform:"windows",secureStorage:true,systemTunnel:true,killSwitch:true}};
    w.__TAURI_INTERNALS__={invoke:async(command:string,args?:{enabled:boolean})=>{
      if(command==="update_monitor_mode")return false;
      if(command==="tunnel_status")return{phase:"connected",ownedByCaller:true,error:null};
      if(command==="set_egress_guard"){enabled=args!.enabled;w.guardChanges++;}
      if(command==="set_egress_guard"||command==="egress_guard_status")return{enabled,filtering:enabled,ownedByCaller:enabled};
      return state;
    }};
  });
  await page.goto("/");
  await page.locator('[data-page="settings"]').first().click();
  await expect(page.getByRole("button",{name:"Enable kill switch…",exact:true})).toBeEnabled();
  await page.getByRole("button",{name:"Enable kill switch…",exact:true}).click();
  const dialog=page.locator("#guard-confirm");
  await expect(dialog).toContainText("After Disconnect, Exit and service restart");
  expect(await page.evaluate(()=>(window as unknown as {guardChanges:number}).guardChanges)).toBe(0);
  await dialog.getByRole("button",{name:"I understand, enable",exact:true}).click();
  await expect(page.getByRole("button",{name:"Exit and keep network blocked",exact:true})).toBeVisible();
  expect(await page.evaluate(()=>(window as unknown as {guardChanges:number}).guardChanges)).toBe(1);
  await page.getByRole("button",{name:"Release Moth network block",exact:true}).click();
  await expect(page.locator("#guard-confirm")).toContainText("direct egress protection will be removed");
  await page.locator("#guard-confirm").getByRole("button",{name:"Cancel",exact:true}).click();
  expect(await page.evaluate(()=>(window as unknown as {guardChanges:number}).guardChanges)).toBe(1);
});
