import {test,expect} from "@playwright/test";
test("proxy status is distinct and access credentials are shown only on request",async({page})=>{
  await page.addInitScript(()=>{
    const w=window as unknown as {isTauri:boolean;__TAURI_INTERNALS__:unknown;accessCalls:number;proxySaves:number;startupCalls:number};
    w.isTauri=true;w.accessCalls=0;w.proxySaves=0;w.startupCalls=0;
    let startup=false;
    const settings={mtu:1500,ipv6:true,dns:"https://1.1.1.1/dns-query",bootstrapDns:"https://1.1.1.1/dns-query",rules:[],splitMode:"bypass",splitApps:[],healthEnabled:false,healthIntervalSeconds:300,healthUrl:"https://cp.cloudflare.com/generate_204",autoReconnect:false,failover:false,autoConnect:false,theme:"light",language:"en"};
    const state={profiles:[],selected:null,version:"0.3.5",engines:[],settings,proxyPreferences:{mode:"proxy",shareWithTun:false,systemProxy:false,listener:{listen:"127.0.0.1",port:10808,allowedClients:[]}},capabilities:{platform:"windows",secureStorage:true,systemTunnel:true,killSwitch:true}};
    w.__TAURI_INTERNALS__={invoke:async(command:string,args?:{enabled:boolean})=>{
      if(command==="update_monitor_mode")return false;
      if(command==="tunnel_status")return{mode:"proxy",phase:"connected",ownedByCaller:true,error:null};
      if(command==="egress_guard_status")return{enabled:false,filtering:false,ownedByCaller:false};
      if(command==="system_proxy_status")return{supported:true,recoveryPending:false,owned:false};
      if(command==="set_autostart"){startup=args!.enabled;w.startupCalls++;}
      if(command==="autostart_status"||command==="set_autostart")return{supported:true,registered:startup,owned:startup,currentTarget:startup,conflict:false};
      if(command==="proxy_access"){w.accessCalls++;return{address:"127.0.0.1",port:10808,username:"moth",password:"synthetic-access-secret"};}
      if(command==="save_proxy_preferences")w.proxySaves++;
      return state;
    }};
  });
  await page.goto("/");
  await expect(page.getByRole("heading",{name:"HTTP/SOCKS proxy connected",exact:true})).toBeVisible();
  await expect(page.locator(".protection-note")).toContainText("no system TUN");
  await page.locator('[data-page="settings"]').click();
  expect(await page.evaluate(()=>(window as unknown as {accessCalls:number}).accessCalls)).toBe(0);
  await expect(page.getByText("synthetic-access-secret",{exact:true})).toHaveCount(0);
  await page.locator("[data-autostart]").check();
  expect(await page.evaluate(()=>(window as unknown as {startupCalls:number}).startupCalls)).toBe(1);
  await expect(page.locator('[name="autoConnect"]')).not.toBeChecked();
  await page.getByRole("button",{name:"Show access details…",exact:true}).click();
  const dialog=page.locator("#proxy-access");await expect(dialog).toBeVisible();
  await expect(dialog.getByLabel("Password",{exact:true})).toHaveValue("synthetic-access-secret");
  await dialog.getByRole("button",{name:"Close",exact:true}).click();await expect(dialog).toHaveCount(0);
  await page.locator('[data-proxy-form] [name="address"]').fill("192.168.1.5");
  page.once("dialog",d=>d.dismiss());
  await page.getByRole("button",{name:"Save mode",exact:true}).click();
  expect(await page.evaluate(()=>(window as unknown as {proxySaves:number}).proxySaves)).toBe(0);
});
