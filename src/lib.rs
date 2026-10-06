//! Control ASUS monitors over DDC/CI on Linux and Windows.
//!
//! Settings are standard DDC/CI (VESA MCCS) plus ASUS's private VCP codes (0xDC, 0xE0-0xFF),
//! reverse engineered from ASUS DisplayWidget Center. See `docs/PROTOCOL.md`.

mod caps;
mod features;
pub mod lightbar;
mod monitor;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(not(any(target_os = "linux", windows)))]
compile_error!("asusdisplay only supports Linux (i2c-dev) and Windows (dxva2)");

pub use caps::Capabilities;
pub use features::{Choice, Feature, Kind, ProductLine};
pub use monitor::{Monitor, Reading, Support, parse_number};

pub type Result<T> = std::result::Result<T, String>;

/// Reply to a "Get VCP Feature" request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Vcp {
    pub current: u16,
    pub max: u16,
}

/// DDC/CI transport to one monitor.
pub trait Ddc: Send {
    fn get(&mut self, code: u8) -> Result<Vcp>;
    fn set(&mut self, code: u8, value: u16) -> Result<()>;
    /// MCCS capabilities string. Slow: up to a few seconds.
    fn capabilities(&mut self) -> Result<String>;
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Use this `/dev/i2c-N` and skip discovery.
    pub bus: Option<u32>,
    /// Scales the DDC/CI delays. Raise it for monitors that answer slowly.
    pub sleep_multiplier: f64,
    /// Print bus discovery to stderr.
    pub verbose: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { bus: None, sleep_multiplier: 1.0, verbose: false }
    }
}

/// Every monitor that answers DDC/CI, ASUS first.
pub fn enumerate(options: &Options) -> Vec<Monitor> {
    #[cfg(target_os = "linux")]
    let mut monitors = linux::enumerate(options);
    #[cfg(windows)]
    let mut monitors = {
        let _ = options;
        windows::enumerate()
    };

    monitors.sort_by_key(|m| (!m.is_asus(), m.id.to_lowercase()));
    monitors
}

/// The parts of an EDID we use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edid {
    /// PNP id, e.g. "AUS".
    pub manufacturer: String,
    pub name: Option<String>,
    pub serial: Option<String>,
}

impl Edid {
    pub fn parse(data: &[u8]) -> Option<Edid> {
        if data.len() < 128 || data[..8] != [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00] {
            return None;
        }

        // Bytes 8-9: three 5-bit letters, 'A' = 1.
        let id = u16::from_be_bytes([data[8], data[9]]);
        let manufacturer = [10, 5, 0].iter().map(|s| char::from(b'@' + (id >> s & 0x1F) as u8)).collect();
        let mut edid = Edid { manufacturer, name: None, serial: None };

        // Display descriptors: 18 bytes each, starting 00 00 00 <tag>, text at offset 5.
        for d in data[54..126].chunks(18).filter(|d| d[..3] == [0, 0, 0]) {
            let text = String::from_utf8_lossy(d[5..].split(|&b| b == b'\n').next().unwrap_or_default()).trim().to_string();
            match d[3] {
                0xFC => edid.name = Some(text),
                0xFF => edid.serial = Some(text),
                _ => {}
            }
        }

        Some(edid)
    }
}

#[cfg(test)]
mod tests {
    use super::Edid;

    #[test]
    fn edid() {
        let mut e = [0u8; 128];
        e[..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        e[8..10].copy_from_slice(&((1u16 << 10) | (21 << 5) | 19).to_be_bytes()); // "AUS"
        for (offset, tag, text) in [(54, 0xFC, "XG27ACDMS\n   "), (72, 0xFF, "SERIAL123\n   ")] {
            e[offset + 3] = tag;
            e[offset + 5..offset + 18].copy_from_slice(text.as_bytes());
        }

        let edid = Edid::parse(&e).unwrap();
        assert_eq!(edid.manufacturer, "AUS");
        assert_eq!(edid.name.as_deref(), Some("XG27ACDMS"));
        assert_eq!(edid.serial.as_deref(), Some("SERIAL123"));
        assert!(Edid::parse(&[0; 128]).is_none());
    }
}
