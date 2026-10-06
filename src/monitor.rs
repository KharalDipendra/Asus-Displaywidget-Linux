use std::thread::sleep;
use std::time::Duration;

use serde::Serialize;

use crate::caps::{self, Capabilities};
use crate::features::{self, Feature, Kind, ProductLine, Table};
use crate::{Ddc, Edid, Result, Vcp};

/// A monitor's current value for one feature, ready to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Reading {
    pub current: u16,
    pub max: u16,
    pub text: String,
    /// Locked in the current mode (ASUS firmware answers 254), or nothing to adjust.
    pub unavailable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
}

/// A feature the monitor doesn't expose, and why (for the dev "unsupported" list).
#[derive(Debug, Clone, Serialize)]
pub struct Support {
    #[serde(flatten)]
    pub feature: Feature,
    pub reason: String,
}

pub struct Monitor {
    /// `/dev/i2c-N` on Linux, `\\.\DISPLAY1` on Windows.
    pub id: String,
    pub connector: Option<String>,
    pub model: String,
    pub edid: Option<Edid>,
    ddc: Box<dyn Ddc>,
    caps: Option<Result<Capabilities>>,
    /// The 0xEF reply: read once, used for both the table version and the ASUS vendor check.
    version: Option<Option<Vcp>>,
}

impl Monitor {
    pub(crate) fn new(
        ddc: Box<dyn Ddc>,
        id: String,
        connector: Option<String>,
        edid: Option<Edid>,
        os_name: Option<String>,
    ) -> Monitor {
        let model = edid.as_ref().and_then(|e| e.name.clone()).or(os_name).unwrap_or_else(|| "unknown".into());
        Monitor { id, connector, model, edid, ddc, caps: None, version: None }
    }

    pub fn is_asus(&self) -> bool {
        self.edid.as_ref().is_some_and(|e| e.manufacturer == "AUS")
            || ProductLine::from_model(&self.model) != ProductLine::Unknown
    }

    pub fn product_line(&self) -> ProductLine {
        ProductLine::from_model(&self.model)
    }

    pub fn is_oled(&self) -> bool {
        features::is_oled(&self.model)
    }

    /// Read once per `Monitor`; it takes a second or two. Falls back to the strings DisplayWidget
    /// Center ships for monitors that don't answer.
    pub fn capabilities(&mut self) -> Result<&Capabilities> {
        let (ddc, model) = (&mut self.ddc, &self.model);
        let caps = self.caps.get_or_insert_with(|| {
            let raw = ddc.capabilities().or_else(|e| caps::fallback(model).map(str::to_string).ok_or(e));
            raw.map(|raw| Capabilities::parse(&raw))
        });
        caps.as_ref().map_err(Clone::clone)
    }

    fn version(&mut self) -> Option<Vcp> {
        let ddc = &mut self.ddc;
        *self.version.get_or_insert_with(|| ddc.get(features::TABLE_VERSION).ok())
    }

    /// `VCPAPI.GetVCPVersion`: 0xEF max high byte 2 means the new ASUS VCP table.
    pub fn new_table(&mut self) -> bool {
        self.version().is_some_and(|v| v.max >> 8 == 2)
    }

    /// `VCPAPI.IsASUSVendor`: the 0xEF current value is in the ASUS vendor set. Gates the private codes.
    pub fn is_asus_vcp(&mut self) -> bool {
        self.version().is_some_and(|v| features::is_asus_vendor(v.current))
    }

    pub fn get_vcp(&mut self, code: u8) -> Result<Vcp> {
        self.ddc.get(code)
    }

    pub fn set_vcp(&mut self, code: u8, value: u16) -> Result<()> {
        self.ddc.set(code, value)
    }

    fn table_features(&mut self) -> Vec<Feature> {
        let table = if self.new_table() { Table::New } else { Table::Legacy };
        features::all(&self.model).into_iter().filter(|f| f.table == Table::Any || f.table == table).collect()
    }

    /// Supported features for this monitor, in display order.
    pub fn features(&mut self, all: bool) -> Vec<Feature> {
        let mut list: Vec<Feature> = if all {
            self.table_features()
        } else {
            self.table_features().into_iter().filter(|f| self.support_reason(f).is_none()).collect()
        };
        list.sort_by_key(Feature::group_rank);
        list
    }

    /// Features the monitor doesn't expose, each with the reason (for the dev tools view).
    pub fn unsupported(&mut self) -> Vec<Support> {
        let mut list: Vec<Support> = self
            .table_features()
            .into_iter()
            .filter_map(|f| self.support_reason(&f).map(|reason| (f, reason)))
            .map(|(feature, reason)| Support { feature, reason })
            .collect();
        list.sort_by_key(|s| s.feature.group_rank());
        list
    }

    pub fn feature(&mut self, key: &str) -> Option<Feature> {
        self.table_features().into_iter().find(|f| f.key.eq_ignore_ascii_case(key))
    }

    /// `None` if the monitor exposes this feature, otherwise why it doesn't.
    pub fn support_reason(&mut self, f: &Feature) -> Option<String> {
        if features::is_private(f.code) && !self.is_asus_vcp() {
            return Some("ASUS-only setting; this isn't an ASUS monitor".into());
        }
        if f.hide_oled && self.is_oled() {
            return Some("not used on OLED panels".into());
        }

        // Without a capabilities string the monitor gets the final say (reads will tell).
        let Ok(caps) = self.capabilities() else { return None };
        let Some(values) = caps.vcp.get(&f.code) else {
            return Some(format!("not advertised (VCP 0x{:02X})", f.code));
        };
        if f.kind == Kind::Toggle && values.len() == 1 && values[0] & f.bit == 0 {
            return Some("not in the monitor's toggle set".into());
        }
        if !f.need.is_empty() && !f.need.iter().any(|v| values.contains(v)) {
            return Some("not available on this monitor".into());
        }
        if f.avoid.iter().any(|v| values.contains(v)) {
            return Some("marked not available".into());
        }
        None
    }

    pub fn read(&mut self, f: &Feature) -> Result<Reading> {
        if f.kind == Kind::Action {
            return Err(format!("{} is write-only", f.key));
        }
        let vcp = self.ddc.get(f.code)?;
        let mut reading = describe(f, vcp);
        // RGB gains only apply in the User colour-temperature preset (non-ProArt).
        if matches!(f.key, "red" | "green" | "blue") && self.product_line() != ProductLine::ProArt && !reading.unavailable {
            let user = self.ddc.get(0x14).map(|v| v.current & 0xFF == 0x0B).unwrap_or(true);
            if !user {
                reading.unavailable = true;
                reading.text = "set Color Temperature to User".into();
            }
        }
        Ok(reading)
    }

    /// Accepts a number (decimal or 0x hex), a choice name, `on`/`off`/`toggle` for toggles,
    /// and `+N`/`-N` steps for sliders. `force` allows disruptive features and values the
    /// monitor doesn't advertise.
    pub fn write(&mut self, f: &Feature, input: &str, force: bool) -> Result<()> {
        if f.kind == Kind::Info {
            return Err(format!("{} is read-only", f.key));
        }
        if f.force && !force {
            return Err(format!("{}: {} Use --force to do it anyway.", f.key, f.note.unwrap_or("disruptive.")));
        }

        let input = input.trim();
        match f.kind {
            // Sliders are checked against the monitor's own range, so they never wait for capabilities.
            Kind::Continuous => {
                let value = self.slider_value(f, input)?;
                self.ddc.set(f.code, value)?;
                // Blue Light Filter interacts with brightness/colour; let the panel settle before a re-read.
                if f.code == 0xE6 {
                    sleep(Duration::from_millis(100));
                }
            }
            Kind::Toggle => {
                if !force && self.support_reason(f).is_some() {
                    return Err(format!("{} does not support {}", self.model, f.key));
                }
                let value = self.toggle_value(f, input)?;
                self.ddc.set(f.code, value)?;
            }
            _ => {
                let value = self.choice_value(f, input, force)?;
                self.write_choice(f, value)?;
            }
        }
        Ok(())
    }

    fn slider_value(&mut self, f: &Feature, input: &str) -> Result<u16> {
        let Vcp { current, max } = self.ddc.get(f.code)?;
        let number = |s: &str| parse_number(s).ok_or_else(|| format!("{} needs a number, got '{input}'", f.key));
        let target = if let Some(step) = input.strip_prefix('+') {
            i64::from(current) + number(step)?
        } else if let Some(step) = input.strip_prefix('-') {
            i64::from(current) - number(step)?
        } else {
            let n = number(input)?;
            if n > i64::from(max) {
                return Err(format!("{} goes from 0 to {max}", f.key));
            }
            n
        };
        Ok(target.clamp(0, i64::from(max)) as u16)
    }

    /// `VCPAPI.SetToggleSettingsItem`: read 0xFD, flip one bit, write it back.
    fn toggle_value(&mut self, f: &Feature, input: &str) -> Result<u16> {
        let current = self.ddc.get(f.code)?.current;
        let on = match input.to_ascii_lowercase().as_str() {
            "on" | "1" | "true" => true,
            "off" | "0" | "false" => false,
            "toggle" => current & f.bit == 0,
            _ => return Err(format!("{} takes on, off or toggle", f.key)),
        };
        Ok(if on { current | f.bit } else { current & !f.bit })
    }

    fn choice_value(&mut self, f: &Feature, input: &str, force: bool) -> Result<u16> {
        let mut value = match f.choices.iter().find(|c| c.name.eq_ignore_ascii_case(input)) {
            Some(choice) => choice.raw,
            None => parse_number(input).and_then(|n| u16::try_from(n).ok()).ok_or_else(|| {
                let names: Vec<_> = f.choices.iter().map(|c| c.name.as_str()).collect();
                format!("{} takes one of: {}", f.key, names.join(", "))
            })?,
        };

        // The app never writes HDR "off"; you turn HDR off in the OS instead.
        if f.key == "hdr" && value == 0 && !force {
            return Err("can't switch HDR off over DDC/CI; turn off HDR in your display settings".into());
        }

        if force {
            return Ok(value);
        }

        let model = self.model.clone();
        let Ok(caps) = self.capabilities() else { return Ok(value) };
        let declared = caps.vcp.get(&f.code).ok_or_else(|| format!("{model} does not support {}", f.key))?;
        if declared.is_empty() {
            return Ok(value);
        }
        // VCPAPI.SetDisplayMode: calibrated Gaming panels expose sRGB as mode 10.
        if f.code == features::MODE && value == 3 && declared.contains(&0x0A) {
            value = 10;
        }
        // 16-bit values are listed by their low byte (HDR / PIP) or high byte (ProArt presets).
        let listed = |v| declared.contains(&v);
        if listed(value) || value > 0xFF && (listed(value & 0xFF) || listed(value >> 8)) {
            Ok(value)
        } else {
            Err(format!("{model} doesn't list {input} as a value for {}. Use --force to send it anyway.", f.key))
        }
    }

    /// Writes an enum/action value, with the byte-packing, HLG two-step and mode-settle behaviours
    /// the app uses.
    fn write_choice(&mut self, f: &Feature, value: u16) -> Result<()> {
        if f.packed {
            // PIP sources and OSD shortcuts live in one byte of a 16-bit register; keep the other byte.
            let current = self.ddc.get(f.code)?.current;
            return self.ddc.set(f.code, (current & !f.mask) | (value & f.mask));
        }

        // ProArt HLG presets are a two-step write (VCPAPI.SetGameVisualByGroup): group, then 0xFF.
        if f.code == 0xE3 && matches!(value, 0x11FF | 0x12FF) {
            self.ddc.set(f.code, value & 0xFF00)?;
            sleep(Duration::from_millis(100));
        }

        self.ddc.set(f.code, value)?;

        // After a picture-mode change the monitor re-locks controls; let it settle before a re-read.
        if f.code == features::MODE || (f.code == 0xE3 && f.key == "mode") {
            sleep(Duration::from_millis(1000));
        }
        Ok(())
    }
}

fn describe(f: &Feature, vcp: Vcp) -> Reading {
    let mut r = Reading {
        current: vcp.current,
        max: vcp.max,
        text: String::new(),
        unavailable: locked(f, vcp),
        on: None,
        selected: None,
    };

    r.text = match f.kind {
        _ if r.unavailable => "n/a in current mode".into(),
        Kind::Continuous => {
            // Hue reports max 0 but runs 0-100 (VCPAPI.GetHue).
            let max = if f.code == 0x90 && vcp.max == 0 { 100 } else { vcp.max };
            r.max = max;
            format!("{} / {}", vcp.current, max)
        }
        Kind::Toggle => {
            let on = vcp.current & f.bit != 0;
            r.on = Some(on);
            (if on { "on" } else { "off" }).into()
        }
        Kind::Enum => match f.choice_for(vcp.current) {
            Some(choice) => {
                r.selected = Some(choice.name.clone());
                choice.label.clone()
            }
            None => format!("unknown (0x{:04X})", vcp.current),
        },
        Kind::Info if f.code == features::TABLE_VERSION => {
            let kind = if vcp.max >> 8 == 2 { "new" } else { "legacy" };
            format!("{}.{} ({kind})", vcp.max >> 8, vcp.max & 0xFF)
        }
        Kind::Info if f.code == features::FIRMWARE => format!("{} (panel {})", vcp.max, vcp.current & 0xFF),
        Kind::Info if f.code == features::MCU_FIRMWARE => {
            if vcp.current == 254 { "none".into() } else { vcp.current.to_string() }
        }
        Kind::Info if f.code == features::ORIENTATION => match vcp.current {
            1 | 0xF1 => "0°".into(),
            2 | 0xF3 => "90°".into(),
            3 | 0xF2 => "180°".into(),
            4 | 0xF0 => "270°".into(),
            other => format!("0x{other:02X}"),
        },
        Kind::Info if f.code == 0xD5 => format!("{}%", vcp.current),
        _ => vcp.current.to_string(),
    };
    r
}

/// ASUS firmware answers 254 (0xFE) in the active byte for a control that's locked in the current
/// mode; some codes also report max 0. Toggles never carry this.
fn locked(f: &Feature, vcp: Vcp) -> bool {
    if f.kind == Kind::Toggle {
        return false;
    }
    let byte_254 = |v: u16| (f.mask & 0x00FF != 0 && v & 0xFF == 0xFE) || (f.mask & 0xFF00 != 0 && v >> 8 == 0xFE);
    if byte_254(vcp.current) || byte_254(vcp.max) {
        return true;
    }
    // Overdrive and HDR also go dead with max 0; a slider with max 0 has nothing to move.
    match f.kind {
        Kind::Continuous if f.code != 0x90 => vcp.max == 0,
        Kind::Enum if matches!(f.code, 0xE0 | 0xE2) => vcp.max == 0,
        _ => false,
    }
}

/// Decimal or `0x` hex.
pub fn parse_number(s: &str) -> Option<i64> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::caps::XG27ACDMS;

    /// XG27ACDMS values as read from the real monitor.
    struct Fake(HashMap<u8, Vcp>);

    impl Ddc for Fake {
        fn get(&mut self, code: u8) -> Result<Vcp> {
            self.0.get(&code).copied().ok_or_else(|| format!("0x{code:02X} unsupported"))
        }

        fn set(&mut self, code: u8, value: u16) -> Result<()> {
            self.0.get_mut(&code).map(|v| v.current = value).ok_or_else(|| format!("0x{code:02X} unsupported"))
        }

        fn capabilities(&mut self) -> Result<String> {
            Ok(XG27ACDMS.into())
        }
    }

    fn xg27acdms() -> Monitor {
        let values = [
            (0x10, 70, 100), (0x14, 0x0B, 0x0B), (0xDC, 4, 10), (0xE2, 0, 0x0104), (0xE3, 0, 15), (0xE6, 254, 254),
            (0xEC, 0, 1), (0xEF, 0xB1, 0x0217), (0xF3, 0xFE, 0xFE), (0xFD, 0x7869, 0x7879), (0xFF, 0x30, 102),
        ];
        let fake = Fake(values.iter().map(|&(code, current, max)| (code, Vcp { current, max })).collect());
        let edid = Edid { manufacturer: "AUS".into(), name: Some("XG27ACDMS".into()), serial: None };
        Monitor::new(Box::new(fake), "fake".into(), None, Some(edid), None)
    }

    fn write(m: &mut Monitor, key: &str, input: &str, force: bool) -> Result<u16> {
        let f = m.feature(key).unwrap();
        m.write(&f, input, force)?;
        Ok(m.get_vcp(f.code)?.current)
    }

    #[test]
    fn supported_features() {
        let mut m = xg27acdms();
        assert!(m.new_table() && m.is_asus_vcp() && m.is_oled());
        let keys: Vec<_> = m.features(false).iter().map(|f| f.key).collect();
        assert!(keys.contains(&"uniform-brightness") && keys.contains(&"screen-move"));
        assert!(!keys.contains(&"od"), "OLED hides overdrive");
        assert!(!keys.contains(&"ascr"), "OLED hides ASCR");
        assert!(!keys.contains(&"kvm"), "F3(FE) means KVM not available");
    }

    #[test]
    fn non_asus_hides_private_codes() {
        let mut m = xg27acdms();
        m.version = Some(Some(Vcp { current: 1, max: 0 })); // not an ASUS vendor code
        assert!(!m.is_asus_vcp());
        // Private codes (mode, hdr) go; standard MCCS (brightness) stays.
        let reason = |m: &mut Monitor, key: &str| {
            let f = m.feature(key).unwrap();
            m.support_reason(&f)
        };
        assert!(reason(&mut m, "mode").is_some());
        assert!(reason(&mut m, "hdr").is_some());
        assert!(reason(&mut m, "brightness").is_none());
    }

    #[test]
    fn unsupported_list_has_reasons() {
        let mut m = xg27acdms();
        let un = m.unsupported();
        assert!(un.iter().any(|s| s.feature.key == "od" && s.reason.contains("OLED")));
        assert!(un.iter().any(|s| s.feature.key == "kvm"));
        assert!(un.iter().all(|s| !s.reason.is_empty()));
    }

    #[test]
    fn writes() {
        let mut m = xg27acdms();
        assert_eq!(write(&mut m, "uniform-brightness", "off", false), Ok(0x7869 & !0x40));
        assert_eq!(write(&mut m, "uniform-brightness", "toggle", false), Ok(0x7869));
        assert_eq!(write(&mut m, "brightness", "+50", false), Ok(100));
        assert_eq!(write(&mut m, "brightness", "-30", false), Ok(70));
        assert!(write(&mut m, "brightness", "101", false).is_err());
        assert_eq!(write(&mut m, "mode", "srgb", false), Ok(10));
        assert!(write(&mut m, "crosshair", "red-dot", false).is_err(), "E3 lists 00 and 07-0F only");
        assert_eq!(write(&mut m, "crosshair", "red-dot", true), Ok(1));
        assert!(write(&mut m, "reset", "reset", false).is_err());
        assert_eq!(write(&mut m, "reset", "reset", true), Ok(1));
    }

    #[test]
    fn hdr_off_is_read_only() {
        let mut m = xg27acdms();
        let hdr = m.feature("hdr").unwrap();
        assert!(m.write(&hdr, "off", false).is_err());
        assert!(m.write(&hdr, "off", true).is_ok());
    }

    #[test]
    fn readings() {
        let mut m = xg27acdms();
        let mut read = |key: &str| {
            let f = m.feature(key).unwrap();
            m.read(&f).unwrap()
        };
        assert_eq!(read("blue-light").text, "n/a in current mode");
        assert_eq!(read("mode").selected.as_deref(), Some("user"));
        assert_eq!(read("global-dimming").on, Some(true));
        assert_eq!(read("firmware").text, "102 (panel 48)");
        assert_eq!(read("vcp-version").text, "2.23 (new)");
    }
}
