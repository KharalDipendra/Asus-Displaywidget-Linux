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
    /// Locked in the current mode (ASUS firmware answers 254/254), or a slider with nothing to slide.
    pub unavailable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
}

pub struct Monitor {
    /// `/dev/i2c-N`
    pub id: String,
    pub connector: Option<String>,
    pub model: String,
    pub edid: Option<Edid>,
    ddc: Box<dyn Ddc>,
    caps: Option<Result<Capabilities>>,
    new_table: Option<bool>,
}

impl Monitor {
    pub(crate) fn new(ddc: Box<dyn Ddc>, id: String, connector: Option<String>, edid: Option<Edid>) -> Monitor {
        let model = edid.as_ref().and_then(|e| e.name.clone()).unwrap_or_else(|| "unknown".into());
        Monitor { id, connector, model, edid, ddc, caps: None, new_table: None }
    }

    pub fn is_asus(&self) -> bool {
        self.edid.as_ref().is_some_and(|e| e.manufacturer == "AUS")
    }

    pub fn product_line(&self) -> ProductLine {
        ProductLine::from_model(&self.model)
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

    /// `VCPAPI.GetVCPVersion`: 0xEF max high byte 2 means the new ASUS VCP table.
    pub fn new_table(&mut self) -> bool {
        let ddc = &mut self.ddc;
        *self.new_table.get_or_insert_with(|| ddc.get(features::TABLE_VERSION).is_ok_and(|r| r.max >> 8 == 2))
    }

    pub fn get_vcp(&mut self, code: u8) -> Result<Vcp> {
        self.ddc.get(code)
    }

    pub fn set_vcp(&mut self, code: u8, value: u16) -> Result<()> {
        self.ddc.set(code, value)
    }

    /// Features for this monitor's product line and VCP table, in display order.
    /// Unless `all`, only what the capabilities string advertises.
    pub fn features(&mut self, all: bool) -> Vec<Feature> {
        let table = if self.new_table() { Table::New } else { Table::Legacy };
        let mut list: Vec<Feature> = features::all(&self.model)
            .into_iter()
            .filter(|f| f.table == Table::Any || f.table == table)
            .collect();
        if !all {
            list.retain(|f| self.supports(f));
        }
        list.sort_by_key(Feature::group_rank);
        list
    }

    pub fn feature(&mut self, key: &str) -> Option<Feature> {
        self.features(true).into_iter().find(|f| f.key.eq_ignore_ascii_case(key))
    }

    /// True when the capabilities can't be read: the monitor gets the final say.
    pub fn supports(&mut self, f: &Feature) -> bool {
        let Ok(caps) = self.capabilities() else { return true };
        match caps.vcp.get(&f.code) {
            None => false,
            // FD(7879): the one 16-bit token is the mask of implemented toggle bits.
            Some(values) if f.kind == Kind::Toggle && values.len() == 1 => values[0] & f.bit != 0,
            Some(_) => true,
        }
    }

    pub fn read(&mut self, f: &Feature) -> Result<Reading> {
        if f.kind == Kind::Action {
            return Err(format!("{} is write-only", f.key));
        }
        Ok(describe(f, self.ddc.get(f.code)?))
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
        let value = match f.kind {
            // Sliders are checked against the monitor's own range, so they never wait for the capabilities string.
            Kind::Continuous => self.slider_value(f, input)?,
            Kind::Toggle => {
                if !force && !self.supports(f) {
                    return Err(format!("{} does not support {}", self.model, f.key));
                }
                self.toggle_value(f, input)?
            }
            _ => self.choice_value(f, input, force)?,
        };
        self.ddc.set(f.code, value)
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
        if f.code == features::MODE && value == 3 && !declared.contains(&3) && declared.contains(&10) {
            value = 10;
        }
        // 16-bit values are listed by their low byte (HDR modes) or high byte (ProArt presets).
        let listed = |v| declared.contains(&v);
        if listed(value) || value > 0xFF && (listed(value & 0xFF) || listed(value >> 8)) {
            Ok(value)
        } else {
            Err(format!("{model} doesn't list {} as a value for {}. Use --force to send it anyway.", input, f.key))
        }
    }
}

fn describe(f: &Feature, vcp: Vcp) -> Reading {
    let mut r = Reading {
        current: vcp.current,
        max: vcp.max,
        text: String::new(),
        unavailable: f.kind != Kind::Toggle && (vcp.current == 254 && vcp.max == 254 || f.kind == Kind::Continuous && vcp.max == 0),
        on: None,
        selected: None,
    };

    r.text = match f.kind {
        _ if r.unavailable => "n/a in current mode".into(),
        Kind::Continuous => format!("{} / {}", vcp.current, vcp.max),
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
        _ => vcp.current.to_string(),
    };
    r
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
            (0x10, 70, 100), (0xDC, 4, 10), (0xE3, 0, 15), (0xE6, 254, 254), (0xEC, 0, 1),
            (0xEF, 1, 0x0217), (0xFD, 0x7869, 0x7879), (0xFF, 0x30, 102),
        ];
        let fake = Fake(values.iter().map(|&(code, current, max)| (code, Vcp { current, max })).collect());
        let edid = Edid { manufacturer: "AUS".into(), name: Some("XG27ACDMS".into()), serial: None };
        Monitor::new(Box::new(fake), "fake".into(), None, Some(edid))
    }

    fn write(m: &mut Monitor, key: &str, input: &str, force: bool) -> Result<u16> {
        let f = m.feature(key).unwrap();
        m.write(&f, input, force)?;
        Ok(m.get_vcp(f.code)?.current)
    }

    #[test]
    fn features_follow_table_and_capabilities() {
        let mut m = xg27acdms();
        assert!(m.new_table());
        let keys: Vec<_> = m.features(false).iter().map(|f| f.key).collect();
        assert!(keys.contains(&"uniform-brightness") && keys.contains(&"screen-move"));
        assert!(!keys.contains(&"sniper"), "bit 0x4 is not in FD(7879)");
        assert!(!keys.contains(&"od"), "0xE0 is not advertised");
        assert_eq!(m.feature("ascr").unwrap().code, 0xFD, "new table moves ASCR into the toggles");
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
        assert!(write(&mut m, "crosshair", "style1", false).is_err(), "E3 lists 00 and 07-0F only");
        assert_eq!(write(&mut m, "crosshair", "style1", true), Ok(1));
        assert!(write(&mut m, "reset", "reset", false).is_err());
        assert_eq!(write(&mut m, "reset", "reset", true), Ok(1));
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
