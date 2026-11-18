//! Persistent and boot-time WFP filters in a Moth-owned provider/sublayer.
//! Ordinary permits do not override any other firewall provider's deny policy.
use super::{EngineError, Policy};
use std::{net::IpAddr, os::windows::ffi::OsStrExt, path::Path};
use windows_sys::{
    core::GUID,
    Win32::{
        Foundation::*,
        NetworkManagement::{
            IpHelper::ConvertInterfaceAliasToLuid, Ndis::NET_LUID_LH, WindowsFilteringPlatform::*,
        },
        Security::{Authorization::*, GetSecurityDescriptorLength},
        Storage::FileSystem::*,
    },
};
const PROVIDER: GUID = GUID::from_u128(0x717e2dfd_e92e_47b0_a177_c4300c26a535);
const SUBLAYER: GUID = GUID::from_u128(0xe0736d1a_acb3_424a_92c1_7f3f5d733035);
fn key(slot: u16) -> GUID {
    GUID::from_u128(0xa516c9fb_8d31_40f4_895f_e67dd0350000 | u128::from(slot))
}
fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}
fn text(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn check(code: u32) -> Result<(), EngineError> {
    if code == 0 {
        Ok(())
    } else {
        Err(EngineError::Network)
    }
}
struct Engine(HANDLE);
impl Engine {
    fn open() -> Result<Self, EngineError> {
        let mut handle = std::ptr::null_mut();
        unsafe {
            check(FwpmEngineOpen0(
                std::ptr::null(),
                10,
                std::ptr::null(),
                std::ptr::null(),
                &mut handle,
            ))?;
        }
        Ok(Self(handle))
    }
    fn transaction(
        &self,
        work: impl FnOnce() -> Result<(), EngineError>,
    ) -> Result<(), EngineError> {
        unsafe {
            check(FwpmTransactionBegin0(self.0, 0))?;
        }
        let result = work().and_then(|_| unsafe { check(FwpmTransactionCommit0(self.0)) });
        if result.is_err() {
            unsafe {
                FwpmTransactionAbort0(self.0);
            }
        }
        result
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            FwpmEngineClose0(self.0);
        }
    }
}
struct Local(*mut core::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn descriptor(sddl: &str) -> Result<Local, EngineError> {
    let mut value = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text(sddl).as_ptr(),
            SDDL_REVISION_1,
            &mut value,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(EngineError::Network);
    }
    Ok(Local(value))
}
struct App(*mut FWP_BYTE_BLOB);
impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            FwpmFreeMemory0((&mut self.0 as *mut *mut FWP_BYTE_BLOB).cast());
        }
    }
}
fn app(path: &Path) -> Result<App, EngineError> {
    let mut value = std::ptr::null_mut();
    unsafe {
        check(FwpmGetAppIdFromFileName0(
            wide(path.as_os_str()).as_ptr(),
            &mut value,
        ))?;
    }
    Ok(App(value))
}
fn same(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}
fn delete_filters(engine: &Engine) -> Result<(), EngineError> {
    for slot in 1..256 {
        let mut existing = std::ptr::null_mut();
        let result = unsafe { FwpmFilterGetByKey0(engine.0, &key(slot), &mut existing) };
        if result as i32 == FWP_E_FILTER_NOT_FOUND {
            continue;
        }
        check(result)?;
        let ours = unsafe {
            !(*existing).providerKey.is_null()
                && same(&*(*existing).providerKey, &PROVIDER)
                && same(&(*existing).subLayerKey, &SUBLAYER)
        };
        unsafe {
            FwpmFreeMemory0((&mut existing as *mut *mut FWPM_FILTER0).cast());
        }
        if !ours {
            return Err(EngineError::Network);
        }
        unsafe {
            check(FwpmFilterDeleteByKey0(engine.0, &key(slot)))?;
        }
    }
    Ok(())
}
fn condition(
    field: GUID,
    kind: FWP_DATA_TYPE,
    value: FWP_CONDITION_VALUE0_0,
) -> FWPM_FILTER_CONDITION0 {
    FWPM_FILTER_CONDITION0 {
        fieldKey: field,
        matchType: FWP_MATCH_EQUAL,
        conditionValue: FWP_CONDITION_VALUE0 {
            r#type: kind,
            Anonymous: value,
        },
    }
}
fn add(
    engine: &Engine,
    slot: u16,
    layer: GUID,
    flags: u32,
    permit: bool,
    conditions: &mut [FWPM_FILTER_CONDITION0],
) -> Result<(), EngineError> {
    let mut provider = PROVIDER;
    let mut name = text(if permit {
        "Moth egress guard: scoped permit"
    } else {
        "Moth egress guard: deny"
    });
    let filter = FWPM_FILTER0 {
        filterKey: key(slot),
        displayData: FWPM_DISPLAY_DATA0 {
            name: name.as_mut_ptr(),
            description: std::ptr::null_mut(),
        },
        flags,
        providerKey: &mut provider,
        layerKey: layer,
        subLayerKey: SUBLAYER,
        weight: FWP_VALUE0 {
            r#type: FWP_UINT8,
            Anonymous: FWP_VALUE0_0 {
                uint8: if permit { 15 } else { 0 },
            },
        },
        action: FWPM_ACTION0 {
            r#type: if permit {
                FWP_ACTION_PERMIT
            } else {
                FWP_ACTION_BLOCK
            },
            ..Default::default()
        },
        numFilterConditions: conditions.len() as u32,
        filterCondition: conditions.as_mut_ptr(),
        ..Default::default()
    };
    unsafe {
        check(FwpmFilterAdd0(
            engine.0,
            &filter,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        ))
    }
}
fn interface() -> Option<u64> {
    let mut luid = NET_LUID_LH::default();
    if unsafe { ConvertInterfaceAliasToLuid(text("Moth").as_ptr(), &mut luid) } == 0 {
        Some(unsafe { luid.Value })
    } else {
        None
    }
}
pub(super) fn apply(policy: &Policy) -> Result<(), EngineError> {
    let engine = Engine::open()?;
    let acl = descriptor("D:P(A;;GA;;;SY)(A;;GA;;;BA)")?;
    let system = descriptor("D:(A;;CC;;;SY)")?;
    let root = std::env::var_os("ProgramFiles").ok_or(EngineError::Process)?;
    let needs_app = |engine| {
        policy
            .endpoints
            .iter()
            .any(|e| e.engine == engine && !e.address.is_loopback())
    };
    let xray = needs_app(crate::Engine::Xray)
        .then(|| app(&Path::new(&root).join("Moth Desktop/engines/xray.exe")))
        .transpose()?;
    let sing_box = needs_app(crate::Engine::SingBox)
        .then(|| app(&Path::new(&root).join("Moth Desktop/engines/sing-box.exe")))
        .transpose()?;
    let svchost = app(
        &Path::new(&std::env::var_os("SystemRoot").ok_or(EngineError::Process)?)
            .join("System32/svchost.exe"),
    )?;
    let mut system_blob = FWP_BYTE_BLOB {
        size: unsafe { GetSecurityDescriptorLength(system.0) },
        data: system.0.cast(),
    };
    let mut luid = interface();
    engine.transaction(|| {
        let mut name = text("Moth persistent egress guard");
        let provider = FWPM_PROVIDER0 {
            providerKey: PROVIDER,
            displayData: FWPM_DISPLAY_DATA0 {
                name: name.as_mut_ptr(),
                description: std::ptr::null_mut(),
            },
            flags: FWPM_PROVIDER_FLAG_PERSISTENT,
            ..Default::default()
        };
        let result = unsafe { FwpmProviderAdd0(engine.0, &provider, acl.0) };
        if result as i32 != FWP_E_ALREADY_EXISTS {
            check(result)?;
        }
        let mut provider_key = PROVIDER;
        let sublayer = FWPM_SUBLAYER0 {
            subLayerKey: SUBLAYER,
            displayData: provider.displayData,
            flags: FWPM_SUBLAYER_FLAG_PERSISTENT,
            providerKey: &mut provider_key,
            weight: u16::MAX,
            ..Default::default()
        };
        let result = unsafe { FwpmSubLayerAdd0(engine.0, &sublayer, acl.0) };
        if result as i32 != FWP_E_ALREADY_EXISTS {
            check(result)?;
        }
        delete_filters(&engine)?;
        let mut slot = 10;
        for (direction, layers) in [
            [
                FWPM_LAYER_ALE_AUTH_CONNECT_V4,
                FWPM_LAYER_ALE_AUTH_CONNECT_V6,
            ],
            [
                FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V4,
                FWPM_LAYER_ALE_AUTH_RECV_ACCEPT_V6,
            ],
        ]
        .into_iter()
        .enumerate()
        {
            for (family, layer) in layers.into_iter().enumerate() {
                for (boot, flags) in [FWPM_FILTER_FLAG_PERSISTENT, FWPM_FILTER_FLAG_BOOTTIME]
                    .into_iter()
                    .enumerate()
                {
                    add(
                        &engine,
                        (1 + family + boot * 2 + direction * 4) as u16,
                        layer,
                        flags,
                        false,
                        &mut [],
                    )?;
                    let mut loopback = condition(
                        FWPM_CONDITION_FLAGS,
                        FWP_UINT32,
                        FWP_CONDITION_VALUE0_0 {
                            uint32: FWP_CONDITION_FLAG_IS_LOOPBACK,
                        },
                    );
                    loopback.matchType = FWP_MATCH_FLAGS_ALL_SET;
                    add(&engine, slot, layer, flags, true, &mut [loopback])?;
                    slot += 1;
                    if let Some(luid) = luid.as_mut() {
                        add(
                            &engine,
                            slot,
                            layer,
                            flags,
                            true,
                            &mut [condition(
                                FWPM_CONDITION_IP_LOCAL_INTERFACE,
                                FWP_UINT64,
                                FWP_CONDITION_VALUE0_0 { uint64: luid },
                            )],
                        )?;
                        slot += 1;
                    }
                    for endpoint in policy.endpoints.iter().filter(|e| {
                        e.address.is_ipv4() == (family == 0) && !e.address.is_loopback()
                    }) {
                        let mut v6 = FWP_BYTE_ARRAY16 {
                            byteArray16: match endpoint.address {
                                IpAddr::V6(ip) => ip.octets(),
                                _ => [0; 16],
                            },
                        };
                        let remote = match endpoint.address {
                            IpAddr::V4(ip) => condition(
                                FWPM_CONDITION_IP_REMOTE_ADDRESS,
                                FWP_UINT32,
                                FWP_CONDITION_VALUE0_0 {
                                    uint32: u32::from_be_bytes(ip.octets()),
                                },
                            ),
                            IpAddr::V6(_) => condition(
                                FWPM_CONDITION_IP_REMOTE_ADDRESS,
                                FWP_BYTE_ARRAY16_TYPE,
                                FWP_CONDITION_VALUE0_0 {
                                    byteArray16: &mut v6,
                                },
                            ),
                        };
                        let mut conditions = vec![
                            remote,
                            condition(
                                FWPM_CONDITION_IP_REMOTE_PORT,
                                FWP_UINT16,
                                FWP_CONDITION_VALUE0_0 {
                                    uint16: endpoint.port,
                                },
                            ),
                            condition(
                                FWPM_CONDITION_ALE_APP_ID,
                                FWP_BYTE_BLOB_TYPE,
                                FWP_CONDITION_VALUE0_0 {
                                    byteBlob: if endpoint.engine == crate::Engine::SingBox {
                                        sing_box.as_ref().ok_or(EngineError::Integrity)?.0
                                    } else {
                                        xray.as_ref().ok_or(EngineError::Integrity)?.0
                                    },
                                },
                            ),
                            condition(
                                FWPM_CONDITION_ALE_USER_ID,
                                FWP_SECURITY_DESCRIPTOR_TYPE,
                                FWP_CONDITION_VALUE0_0 {
                                    sd: &mut system_blob,
                                },
                            ),
                        ];
                        add(&engine, slot, layer, flags, true, &mut conditions)?;
                        slot += 1;
                    }
                    let (source, destination) = if family == 0 { (68, 67) } else { (546, 547) };
                    add(
                        &engine,
                        slot,
                        layer,
                        flags,
                        true,
                        &mut [
                            condition(
                                FWPM_CONDITION_IP_LOCAL_PORT,
                                FWP_UINT16,
                                FWP_CONDITION_VALUE0_0 { uint16: source },
                            ),
                            condition(
                                FWPM_CONDITION_IP_REMOTE_PORT,
                                FWP_UINT16,
                                FWP_CONDITION_VALUE0_0 {
                                    uint16: destination,
                                },
                            ),
                            condition(
                                FWPM_CONDITION_IP_PROTOCOL,
                                FWP_UINT8,
                                FWP_CONDITION_VALUE0_0 { uint8: 17 },
                            ),
                            condition(
                                FWPM_CONDITION_ALE_APP_ID,
                                FWP_BYTE_BLOB_TYPE,
                                FWP_CONDITION_VALUE0_0 {
                                    byteBlob: svchost.0,
                                },
                            ),
                            condition(
                                FWPM_CONDITION_ALE_USER_ID,
                                FWP_SECURITY_DESCRIPTOR_TYPE,
                                FWP_CONDITION_VALUE0_0 {
                                    sd: &mut system_blob,
                                },
                            ),
                        ],
                    )?;
                    slot += 1;
                    if family == 1 {
                        for (prefix, address) in [(10, "fe80::"), (16, "ff02::")] {
                            for kind in [133, 134, 135, 136] {
                                let address: std::net::Ipv6Addr =
                                    address.parse().map_err(|_| EngineError::InvalidConfig)?;
                                let mut mask = FWP_V6_ADDR_AND_MASK {
                                    addr: address.octets(),
                                    prefixLength: prefix,
                                };
                                add(
                                    &engine,
                                    slot,
                                    layer,
                                    flags,
                                    true,
                                    &mut [
                                        condition(
                                            FWPM_CONDITION_IP_REMOTE_ADDRESS,
                                            FWP_V6_ADDR_MASK,
                                            FWP_CONDITION_VALUE0_0 {
                                                v6AddrMask: &mut mask,
                                            },
                                        ),
                                        condition(
                                            FWPM_CONDITION_IP_PROTOCOL,
                                            FWP_UINT8,
                                            FWP_CONDITION_VALUE0_0 { uint8: 58 },
                                        ),
                                        condition(
                                            FWPM_CONDITION_IP_LOCAL_PORT,
                                            FWP_UINT16,
                                            FWP_CONDITION_VALUE0_0 { uint16: kind },
                                        ),
                                    ],
                                )?;
                                slot += 1;
                            }
                        }
                    }
                }
            }
        }
        // Forwarded container/VM packets do not traverse local ALE connect
        // authorization. They receive a separate owned default-deny layer.
        for (family, layer) in [FWPM_LAYER_IPFORWARD_V4, FWPM_LAYER_IPFORWARD_V6]
            .into_iter()
            .enumerate()
        {
            for (boot, flags) in [FWPM_FILTER_FLAG_PERSISTENT, FWPM_FILTER_FLAG_BOOTTIME]
                .into_iter()
                .enumerate()
            {
                add(
                    &engine,
                    (240 + family + boot * 2) as u16,
                    layer,
                    flags,
                    false,
                    &mut [],
                )?;
            }
        }
        Ok(())
    })
}
pub(super) fn remove() -> Result<(), EngineError> {
    let engine = Engine::open()?;
    engine.transaction(|| {
        delete_filters(&engine)?;
        for result in [
            unsafe { FwpmSubLayerDeleteByKey0(engine.0, &SUBLAYER) },
            unsafe { FwpmProviderDeleteByKey0(engine.0, &PROVIDER) },
        ] {
            if !matches!(
                result as i32,
                0 | FWP_E_SUBLAYER_NOT_FOUND | FWP_E_PROVIDER_NOT_FOUND
            ) {
                check(result)?;
            }
        }
        Ok(())
    })
}
pub(super) fn active() -> Result<bool, EngineError> {
    let engine = Engine::open()?;
    for slot in (1..=8).chain(240..=243) {
        let mut filter = std::ptr::null_mut();
        let code = unsafe { FwpmFilterGetByKey0(engine.0, &key(slot), &mut filter) };
        if code as i32 == FWP_E_FILTER_NOT_FOUND {
            return Ok(false);
        }
        check(code)?;
        let valid = unsafe {
            (*filter).action.r#type == FWP_ACTION_BLOCK
                && (*filter).numFilterConditions == 0
                && !(*filter).providerKey.is_null()
                && same(&*(*filter).providerKey, &PROVIDER)
                && same(&(*filter).subLayerKey, &SUBLAYER)
        };
        unsafe {
            FwpmFreeMemory0((&mut filter as *mut *mut FWPM_FILTER0).cast());
        }
        if !valid {
            return Err(EngineError::Network);
        }
    }
    Ok(true)
}
pub(super) fn replace_file(source: &Path, destination: &Path) -> Result<(), EngineError> {
    if unsafe {
        MoveFileExW(
            wide(source.as_os_str()).as_ptr(),
            wide(destination.as_os_str()).as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(EngineError::Network);
    }
    Ok(())
}
