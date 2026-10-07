//! DDC/CI over `/dev/i2c-N` (kernel module `i2c-dev`).
//!
//! Buses come from `/sys/class/drm` (connected connectors: the `ddc` link for HDMI/DVI, the DP-AUX
//! `i2c-N` child for DisplayPort). The NVIDIA driver has no such links, so GPU-named adapters in
//! `/sys/class/i2c-dev` are probed as well. Other buses (SMBus, RAM SPD) are never touched.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

use crate::{Ddc, Edid, Monitor, Options, Result, Vcp};

const DDC_ADDRESS: u16 = 0x37;
const EDID_ADDRESS: u16 = 0x50;
/// linux/i2c-dev.h
const I2C_SLAVE: libc::c_ulong = 0x0703;
const ATTEMPTS: usize = 4;

pub fn enumerate(options: &Options) -> Vec<Monitor> {
    let log = |message: String| {
        if options.verbose {
            eprintln!("{message}");
        }
    };

    let mut buses: BTreeMap<u32, (Option<String>, Option<Edid>)> = BTreeMap::new();
    // Buses a connected connector owns, so the adapter scan doesn't list the same monitor twice.
    let mut claimed = Vec::new();
    if let Some(bus) = options.bus {
        buses.insert(bus, (None, None));
    } else {
        for conn in children("/sys/class/drm").filter(|p| name_of(p).starts_with("card") && name_of(p).contains('-')) {
            if read_text(&conn.join("status")) != "connected" {
                continue;
            }
            let Some(bus) = connector_bus(&conn) else {
                log(format!("{}: connected, but no I2C bus", name_of(&conn)));
                continue;
            };
            let edid = fs::read(conn.join("edid")).ok().and_then(|data| Edid::parse(&data));
            let connector = name_of(&conn).split_once('-').map(|(_, c)| c.to_string());
            log(format!("{}: i2c-{bus}", name_of(&conn)));
            claimed.extend(ddc_link_bus(&conn));
            buses.insert(bus, (connector, edid));
        }

        for dir in children("/sys/class/i2c-dev") {
            let Some(bus) = parse_bus(&name_of(&dir)) else { continue };
            let adapter = read_text(&dir.join("name"));
            if !buses.contains_key(&bus) && !claimed.contains(&bus) && is_gpu_adapter(&adapter) {
                log(format!("i2c-{bus} ({adapter}): probing"));
                buses.insert(bus, (None, None));
            }
        }
    }

    buses
        .into_iter()
        .filter_map(|(bus, (connector, edid))| {
            let mut ddc = I2c::new(bus, options.sleep_multiplier);
            // EDID probe: write offset 0, read 128 bytes from 0x50. Read-only.
            let edid = edid.or_else(|| {
                let mut data = [0u8; 128];
                ddc.transfer(EDID_ADDRESS, &[0], &mut data, 0).ok().and_then(|()| Edid::parse(&data))
            });
            // Without an EDID nothing is listening, unless the user picked the bus.
            (edid.is_some() || options.bus.is_some()).then(|| Monitor::new(Box::new(ddc), format!("/dev/i2c-{bus}"), connector, edid, None))
        })
        .collect()
}

fn children(dir: &str) -> impl Iterator<Item = std::path::PathBuf> {
    fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path())
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn parse_bus(name: &str) -> Option<u32> {
    name.strip_prefix("i2c-")?.parse().ok()
}

fn ddc_link_bus(conn: &Path) -> Option<u32> {
    fs::canonicalize(conn.join("ddc")).ok().and_then(|target| parse_bus(&name_of(&target)))
}

/// DP-AUX child first: on DisplayPort the `ddc` link (amdgpu's "DM i2c hw bus") reads EDID but
/// fails DDC/CI writes with EIO. HDMI/DVI have no AUX child and use the `ddc` link.
fn connector_bus(conn: &Path) -> Option<u32> {
    children(&conn.to_string_lossy()).find_map(|child| parse_bus(&name_of(&child))).or_else(|| ddc_link_bus(conn))
}

fn is_gpu_adapter(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    let deny = ["smbus", "i801", "piix4", "designware", "synopsys", "spd"];
    let allow = ["nvidia", "amdgpu", "radeon", "i915", "gmbus", "nouveau", "aux", "dpmst"];
    !deny.iter().any(|d| n.contains(d)) && allow.iter().any(|a| n.contains(a))
}

struct I2c {
    path: String,
    file: Option<File>,
    address: Option<u16>,
    sleep_multiplier: f64,
}

impl I2c {
    fn new(bus: u32, sleep_multiplier: f64) -> I2c {
        I2c { path: format!("/dev/i2c-{bus}"), file: None, address: None, sleep_multiplier }
    }

    fn pause(&self, ms: u64) {
        sleep(Duration::from_secs_f64(ms as f64 * self.sleep_multiplier / 1000.0));
    }

    /// Writes `request`, waits `delay_ms`, then fills `reply` (if not empty).
    fn transfer(&mut self, address: u16, request: &[u8], reply: &mut [u8], delay_ms: u64) -> Result<()> {
        let path = &self.path;
        if self.file.is_none() {
            let file = OpenOptions::new().read(true).write(true).open(path).map_err(|e| {
                let hint = if e.kind() == std::io::ErrorKind::PermissionDenied { " (install the udev rule, see README)" } else { "" };
                format!("{path}: {e}{hint}")
            })?;
            self.file = Some(file);
            self.address = None;
        }
        let file = self.file.as_mut().expect("opened above");

        if self.address != Some(address) {
            // SAFETY: plain ioctl on an open fd; I2C_SLAVE takes the address by value.
            if unsafe { libc::ioctl(file.as_raw_fd(), I2C_SLAVE as _, libc::c_ulong::from(address)) } < 0 {
                return Err(format!("{path}: select 0x{address:02X}: {}", std::io::Error::last_os_error()));
            }
            self.address = Some(address);
        }

        file.write_all(request).map_err(|e| format!("{path}: write: {e}"))?;
        if reply.is_empty() {
            return Ok(());
        }
        self.pause(delay_ms);
        let file = self.file.as_mut().expect("opened above");
        file.read_exact(reply).map_err(|e| format!("{}: read: {e}", self.path))
    }

    /// Sends `request` and parses the reply, retrying busy or garbled answers.
    fn ask<T>(&mut self, request: &[u8], reply_len: usize, parse: impl Fn(&[u8]) -> Reply<T>) -> Result<T> {
        let mut reply = vec![0u8; reply_len];
        let mut error = String::new();
        for attempt in 0..ATTEMPTS {
            if attempt > 0 {
                self.pause(50);
            }
            match self.transfer(DDC_ADDRESS, request, &mut reply, 40).map(|()| parse(&reply)) {
                Ok(Reply::Ok(value)) => return Ok(value),
                Ok(Reply::Unsupported) => return Err("not supported by the monitor".into()),
                Ok(Reply::Retry) => error = format!("{}: no valid DDC/CI reply ({reply:02X?})", self.path),
                Err(e) => error = e,
            }
        }
        Err(error)
    }
}

impl Ddc for I2c {
    fn get(&mut self, code: u8) -> Result<Vcp> {
        self.ask(&packet(&[0x01, code]), 11, |r| parse_vcp(r, code))
    }

    fn set(&mut self, code: u8, value: u16) -> Result<()> {
        let [hi, lo] = value.to_be_bytes();
        let result = self.transfer(DDC_ADDRESS, &packet(&[0x03, code, hi, lo]), &mut [], 0);
        self.pause(50);
        result
    }

    fn capabilities(&mut self) -> Result<String> {
        let mut caps = Vec::new();
        // Fragments of up to 32 bytes; an empty one ends the string.
        while caps.len() < 4096 {
            let offset = caps.len() as u16;
            let [hi, lo] = offset.to_be_bytes();
            let fragment = self.ask(&packet(&[0xF3, hi, lo]), 38, |r| parse_caps(r, offset))?;
            if fragment.is_empty() {
                break;
            }
            caps.extend(fragment);
            self.pause(50);
        }
        Ok(String::from_utf8_lossy(&caps).trim_end_matches(['\0', ' ']).to_string())
    }
}

enum Reply<T> {
    Ok(T),
    Unsupported,
    /// Busy (null message), garbled or not for us.
    Retry,
}

/// Request bytes after the I2C address: source 0x51, length, payload, XOR checksum seeded with 0x6E.
fn packet(payload: &[u8]) -> Vec<u8> {
    let mut p = vec![0x51, 0x80 | payload.len() as u8];
    p.extend_from_slice(payload);
    p.push(p.iter().fold(0x6E, |chk, b| chk ^ b));
    p
}

/// Checks framing and checksum (seeded with 0x50), returns the payload.
fn payload(r: &[u8]) -> Option<&[u8]> {
    let len = (*r.get(1)? & 0x7F) as usize;
    if r[0] != 0x6E || r[1] & 0x80 == 0 || len == 0 || r.len() < len + 3 {
        return None;
    }
    let checksum = r[..len + 2].iter().fold(0x50, |chk, b| chk ^ b);
    (checksum == r[len + 2]).then(|| &r[2..len + 2])
}

/// 02 result code type mh ml sh sl
fn parse_vcp(r: &[u8], code: u8) -> Reply<Vcp> {
    match payload(r) {
        Some(&[0x02, 0x00, c, _, mh, ml, sh, sl]) if c == code => {
            Reply::Ok(Vcp { current: u16::from_be_bytes([sh, sl]), max: u16::from_be_bytes([mh, ml]) })
        }
        Some(&[0x02, 0x01, c, ..]) if c == code => Reply::Unsupported,
        _ => Reply::Retry,
    }
}

/// E3 offset-hi offset-lo data...
fn parse_caps(r: &[u8], offset: u16) -> Reply<Vec<u8>> {
    match payload(r) {
        Some([0xE3, hi, lo, data @ ..]) if u16::from_be_bytes([*hi, *lo]) == offset => Reply::Ok(data.to_vec()),
        _ => Reply::Retry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(body: &[u8]) -> Vec<u8> {
        let mut r = body.to_vec();
        r.push(body.iter().fold(0x50, |chk, b| chk ^ b));
        r
    }

    #[test]
    fn requests() {
        assert_eq!(packet(&[0x01, 0x10]), [0x51, 0x82, 0x01, 0x10, 0xAC], "the spec's get-brightness example");
        assert_eq!(packet(&[0x03, 0x10, 0x00, 0x32])[..6], [0x51, 0x84, 0x03, 0x10, 0x00, 0x32]);
    }

    #[test]
    fn replies() {
        let ok = reply(&[0x6E, 0x88, 0x02, 0x00, 0x10, 0x00, 0x00, 0x64, 0x00, 0x46]);
        assert!(matches!(parse_vcp(&ok, 0x10), Reply::Ok(Vcp { current: 70, max: 100 })));
        assert!(matches!(parse_vcp(&ok, 0x12), Reply::Retry), "reply for another code");
        let mut bad = ok.clone();
        bad[10] ^= 0xFF;
        assert!(matches!(parse_vcp(&bad, 0x10), Reply::Retry), "checksum");
        assert!(matches!(parse_vcp(&[0x6E, 0x80, 0xBE, 0, 0, 0, 0, 0, 0, 0, 0], 0x10), Reply::Retry), "null message");
        let unsupported = reply(&[0x6E, 0x88, 0x02, 0x01, 0xE8, 0, 0, 0, 0, 0]);
        assert!(matches!(parse_vcp(&unsupported, 0xE8), Reply::Unsupported));

        let fragment = reply(&[0x6E, 0x86, 0xE3, 0x00, 0x20, b'(', b'p', b'r']);
        assert!(matches!(parse_caps(&fragment, 0x20), Reply::Ok(d) if d == b"(pr"));
        assert!(matches!(parse_caps(&fragment, 0x21), Reply::Retry), "wrong offset");
        assert!(matches!(parse_caps(&reply(&[0x6E, 0x83, 0xE3, 0x00, 0x23]), 0x23), Reply::Ok(d) if d.is_empty()));
    }

    #[test]
    fn gpu_adapters_only() {
        assert!(is_gpu_adapter("NVIDIA i2c adapter 1 at 1:00.0"));
        assert!(is_gpu_adapter("AMDGPU DM aux hw bus 2"));
        assert!(!is_gpu_adapter("SMBus I801 adapter at efa0"));
        assert!(!is_gpu_adapter("Synopsys DesignWare I2C adapter"));
    }
}
