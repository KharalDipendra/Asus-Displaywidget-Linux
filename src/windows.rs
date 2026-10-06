//! dxva2, the monitor configuration API DisplayWidget Center itself uses. Names come from
//! QueryDisplayConfig, the EDID from `HKLM\SYSTEM\CurrentControlSet\Enum\DISPLAY\...`.

use std::collections::HashMap;
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows_sys::Win32::Devices::Display::*;
use windows_sys::Win32::Foundation::{GetLastError, HANDLE, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW};
use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_BINARY, RegGetValueW};

use crate::{Ddc, Edid, Monitor, Vcp};

const ERROR_GRAPHICS_DDCCI_VCP_NOT_SUPPORTED: u32 = 0xC026_2584;

pub fn enumerate() -> Vec<Monitor> {
    let names = display_names();
    let mut monitors = Vec::new();

    for hmonitor in hmonitors() {
        // SAFETY: zeroed POD structs sized for the calls; hmonitor comes from EnumDisplayMonitors.
        unsafe {
            let mut info: MONITORINFOEXW = zeroed();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            GetMonitorInfoW(hmonitor, (&raw mut info).cast());
            let gdi_name = from_wide(&info.szDevice);

            let mut count = 0;
            if GetNumberOfPhysicalMonitorsFromHMONITOR(hmonitor, &mut count) == 0 {
                continue;
            }
            let mut physical = vec![zeroed::<PHYSICAL_MONITOR>(); count as usize];
            if GetPhysicalMonitorsFromHMONITOR(hmonitor, count, physical.as_mut_ptr()) == 0 {
                continue;
            }

            let (friendly, edid) = names.get(&gdi_name).cloned().unwrap_or_default();
            for (i, p) in physical.iter().enumerate() {
                let id = if count > 1 { format!("{gdi_name}#{i}") } else { gdi_name.clone() };
                let ddc = Dxva { handle: p.hPhysicalMonitor, idle_since: Instant::now() };
                monitors.push(Monitor::new(Box::new(ddc), id, None, edid.clone(), friendly.clone()));
            }
        }
    }

    monitors
}

fn hmonitors() -> Vec<HMONITOR> {
    unsafe extern "system" fn push(hmonitor: HMONITOR, _: HDC, _: *mut RECT, list: LPARAM) -> i32 {
        // SAFETY: `list` is the Vec passed below, alive for the whole call.
        unsafe { (*(list as *mut Vec<HMONITOR>)).push(hmonitor) };
        1
    }

    let mut list = Vec::new();
    // SAFETY: the callback only runs during this call.
    unsafe { EnumDisplayMonitors(null_mut(), null(), Some(push), &raw mut list as LPARAM) };
    list
}

/// `\\.\DISPLAYn` -> (friendly name, EDID), for every active display path.
fn display_names() -> HashMap<String, (Option<String>, Option<Edid>)> {
    let mut names = HashMap::new();
    // SAFETY: the documented two-call pattern with POD buffers of the reported sizes.
    unsafe {
        let (mut path_count, mut mode_count) = (0, 0);
        if GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count) != 0 {
            return names;
        }
        let mut paths = vec![zeroed::<DISPLAYCONFIG_PATH_INFO>(); path_count as usize];
        let mut modes = vec![zeroed::<DISPLAYCONFIG_MODE_INFO>(); mode_count as usize];
        if QueryDisplayConfig(QDC_ONLY_ACTIVE_PATHS, &mut path_count, paths.as_mut_ptr(), &mut mode_count, modes.as_mut_ptr(), null_mut()) != 0 {
            return names;
        }

        for path in &paths[..path_count as usize] {
            let mut source: DISPLAYCONFIG_SOURCE_DEVICE_NAME = zeroed();
            source.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME;
            source.header.size = size_of_val(&source) as u32;
            source.header.adapterId = path.sourceInfo.adapterId;
            source.header.id = path.sourceInfo.id;

            let mut target: DISPLAYCONFIG_TARGET_DEVICE_NAME = zeroed();
            target.header.r#type = DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME;
            target.header.size = size_of_val(&target) as u32;
            target.header.adapterId = path.targetInfo.adapterId;
            target.header.id = path.targetInfo.id;

            if DisplayConfigGetDeviceInfo(&mut source.header) == 0 && DisplayConfigGetDeviceInfo(&mut target.header) == 0 {
                let friendly = Some(from_wide(&target.monitorFriendlyDeviceName)).filter(|n| !n.is_empty());
                let edid = registry_edid(&from_wide(&target.monitorDevicePath));
                names.insert(from_wide(&source.viewGdiDeviceName), (friendly, edid));
            }
        }
    }
    names
}

/// `\\?\DISPLAY#AUS27C0#5&1234&0&UID4353#{guid}` -> `Enum\DISPLAY\AUS27C0\5&1234&0&UID4353\Device Parameters\EDID`
fn registry_edid(device_path: &str) -> Option<Edid> {
    let mut parts = device_path.split('#').skip(1);
    let key = format!(r"SYSTEM\CurrentControlSet\Enum\DISPLAY\{}\{}\Device Parameters", parts.next()?, parts.next()?);
    let mut data = [0u8; 512];
    let mut size = data.len() as u32;
    // SAFETY: NUL-terminated wide strings; `size` describes `data`.
    let status = unsafe {
        RegGetValueW(HKEY_LOCAL_MACHINE, wide(&key).as_ptr(), wide("EDID").as_ptr(), RRF_RT_REG_BINARY, null_mut(), data.as_mut_ptr().cast(), &mut size)
    };
    if status == 0 { Edid::parse(&data[..size as usize]) } else { None }
}

fn from_wide(s: &[u16]) -> String {
    String::from_utf16_lossy(&s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())])
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn last_error() -> String {
    // SAFETY: no preconditions.
    std::io::Error::from_raw_os_error(unsafe { GetLastError() } as i32).to_string()
}

struct Dxva {
    handle: HANDLE,
    /// When the last command finished. DDC/CI wants 50 ms between commands and dxva2 leaves that to
    /// the caller (DisplayWidget Center sleeps before every call too); some monitors fail without it.
    idle_since: Instant,
}

// SAFETY: the handle is a plain token that dxva2 accepts from any thread; `&mut self` keeps calls serial.
unsafe impl Send for Dxva {}

impl Dxva {
    fn call(&mut self, api: impl FnOnce(HANDLE) -> i32) -> bool {
        sleep(Duration::from_millis(50).saturating_sub(self.idle_since.elapsed()));
        let ok = api(self.handle) != 0;
        self.idle_since = Instant::now();
        ok
    }
}

impl Ddc for Dxva {
    fn get(&mut self, code: u8) -> crate::Result<Vcp> {
        let mut error = String::new();
        for _ in 0..3 {
            let (mut kind, mut current, mut max) = (0, 0, 0);
            // SAFETY: out-pointers to locals.
            if self.call(|h| unsafe { GetVCPFeatureAndVCPFeatureReply(h, code, &mut kind, &mut current, &mut max) }) {
                return Ok(Vcp { current: current as u16, max: max as u16 });
            }
            // SAFETY: no preconditions.
            if unsafe { GetLastError() } == ERROR_GRAPHICS_DDCCI_VCP_NOT_SUPPORTED {
                return Err("not supported by the monitor".into());
            }
            error = last_error();
        }
        Err(error)
    }

    fn set(&mut self, code: u8, value: u16) -> crate::Result<()> {
        // SAFETY: valid physical monitor handle.
        if self.call(|h| unsafe { SetVCPFeature(h, code, value.into()) }) { Ok(()) } else { Err(last_error()) }
    }

    fn capabilities(&mut self) -> crate::Result<String> {
        let mut len = 0;
        // SAFETY: out-pointer to a local, then a buffer of exactly `len` bytes.
        if !self.call(|h| unsafe { GetCapabilitiesStringLength(h, &mut len) }) {
            return Err(last_error());
        }
        let mut buf = vec![0u8; len as usize];
        if !self.call(|h| unsafe { CapabilitiesRequestAndCapabilitiesReply(h, buf.as_mut_ptr(), len) }) {
            return Err(last_error());
        }
        Ok(String::from_utf8_lossy(&buf).trim_end_matches('\0').to_string())
    }
}

impl Drop for Dxva {
    fn drop(&mut self) {
        // SAFETY: the handle came from GetPhysicalMonitorsFromHMONITOR and is destroyed once.
        unsafe { DestroyPhysicalMonitor(self.handle) };
    }
}
