//! The feature map, reverse engineered from ASUS DisplayWidget Center 1.3.0.4
//! (VCPAPI.cs, AirVisionVCPCode.cs, VCPColorTemperature.cs, CapabilityConfigManager.cs and the
//! GameVisual / GamePlus / OLED / System pages). docs/PROTOCOL.md has the evidence for each entry.

use serde::Serialize;

pub const MODE: u8 = 0xDC;
pub const TABLE_VERSION: u8 = 0xEF;
pub const FIRMWARE: u8 = 0xFF;
pub const TOGGLES: u8 = 0xFD;
pub const ORIENTATION: u8 = 0xAA;
pub const MCU_FIRMWARE: u8 = 0x79;

/// Order of groups in `status` and the GUI.
const GROUPS: [&str; 10] = ["Picture", "Color", "HDR", "Gaming", "GamePlus", "OLED Care", "System", "PIP", "OSD", "Info"];

/// Port of `Utils.GetProductLine`: decides which picture-mode table applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ProductLine {
    Gaming,
    MainStream,
    Portable,
    ProArt,
    Unknown,
}

impl ProductLine {
    pub fn from_model(model: &str) -> ProductLine {
        let m = model.trim().to_ascii_lowercase();
        if m.len() < 5 {
            return ProductLine::Unknown;
        }
        if m.contains("_d") {
            return ProductLine::Gaming;
        }
        match &m[..2] {
            "vg" | "pg" | "xg" => ProductLine::Gaming,
            "vx" | "vp" | "vy" | "vz" | "vu" | "va" | "vt" | "be" => ProductLine::MainStream,
            "mb" | "mq" => ProductLine::Portable,
            "pa" | "pq" => ProductLine::ProArt,
            _ => ProductLine::Unknown,
        }
    }
}

/// Port of `Utils.IsOledDevice`: name has "mq", or vg/pg/xg with a 'd'/'w' at index 6.
pub fn is_oled(model: &str) -> bool {
    let m = model.trim().to_ascii_lowercase();
    if m.contains("mq") {
        return true;
    }
    let gaming = matches!(m.get(..2), Some("vg" | "pg" | "xg"));
    gaming && matches!(m.as_bytes().get(6), Some(b'd' | b'w'))
}

/// `VCPAPI.IsASUSVendor`: an ASUS monitor answers VCP 0xEF with a *current* value in this set.
pub fn is_asus_vendor(code: u16) -> bool {
    matches!(code,
        0x1A
        | 0x55..=0x58 | 0x66..=0x69 | 0x77..=0x7A | 0x88..=0x8B | 0x99..=0x9C
        | 0xA0..=0xA3 | 0xB1..=0xB4 | 0xC2..=0xC5 | 0xD3..=0xD6 | 0xE4..=0xE7 | 0xF5..=0xF8)
}

/// A code is ASUS-private (shown only on ASUS monitors) when it is 0xDC or 0xE0-0xFD.
pub fn is_private(code: u8) -> bool {
    code == MODE || (0xE0..=0xFD).contains(&code)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Slider from 0 to the maximum the monitor reports.
    Continuous,
    /// One of `choices`.
    Enum,
    /// One bit of the ToggleSettings2 register (0xFD).
    Toggle,
    /// Write-only command.
    Action,
    /// Read-only.
    Info,
}

/// ASUS moved several features when it introduced VCP table version 2 (read from 0xEF).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Table {
    Any,
    New,
    Legacy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Choice {
    pub raw: u16,
    pub name: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Feature {
    pub key: &'static str,
    pub label: &'static str,
    pub code: u8,
    pub kind: Kind,
    pub group: &'static str,
    pub choices: Vec<Choice>,
    /// Applied to the current value before matching `choices`.
    #[serde(skip)]
    pub mask: u16,
    /// Toggle bit inside 0xFD.
    #[serde(skip)]
    pub bit: u16,
    #[serde(skip)]
    pub table: Table,
    /// The choice value occupies one byte (`mask`); writing keeps the other byte (PIP, OSD shortcuts).
    #[serde(skip)]
    pub packed: bool,
    /// The monitor must advertise at least one of these values for the code (KVM: 1-4).
    #[serde(skip)]
    pub need: &'static [u16],
    /// The monitor must advertise none of these values (GamePlus position: 0xFE = "n/a").
    #[serde(skip)]
    pub avoid: &'static [u16],
    /// Hidden on OLED monitors (overdrive, ASCR).
    #[serde(skip)]
    pub hide_oled: bool,
    /// Disruptive: needs `force`.
    pub force: bool,
    pub note: Option<&'static str>,
}

impl Feature {
    fn new(key: &'static str, label: &'static str, code: u8, kind: Kind, group: &'static str) -> Feature {
        Feature {
            key, label, code, kind, group,
            choices: Vec::new(), mask: 0xFFFF, bit: 0, table: Table::Any,
            packed: false, need: &[], avoid: &[], hide_oled: false, force: false, note: None,
        }
    }

    fn slider(key: &'static str, label: &'static str, code: u8, group: &'static str) -> Feature {
        Feature::new(key, label, code, Kind::Continuous, group)
    }

    fn choose(key: &'static str, label: &'static str, code: u8, group: &'static str, choices: Vec<Choice>) -> Feature {
        Feature { choices, ..Feature::new(key, label, code, Kind::Enum, group) }
    }

    fn toggle(key: &'static str, label: &'static str, bit: u16, group: &'static str) -> Feature {
        Feature { bit, table: Table::New, ..Feature::new(key, label, TOGGLES, Kind::Toggle, group) }
    }

    fn action(key: &'static str, label: &'static str, code: u8, group: &'static str, choices: Vec<Choice>) -> Feature {
        Feature { choices, ..Feature::new(key, label, code, Kind::Action, group) }
    }

    fn info(key: &'static str, label: &'static str, code: u8, group: &'static str) -> Feature {
        Feature::new(key, label, code, Kind::Info, group)
    }

    fn mask(self, mask: u16) -> Feature {
        Feature { mask, ..self }
    }

    fn only(self, table: Table) -> Feature {
        Feature { table, ..self }
    }

    fn forced(self, note: &'static str) -> Feature {
        Feature { force: true, note: Some(note), ..self }
    }

    fn note(self, note: &'static str) -> Feature {
        Feature { note: Some(note), ..self }
    }

    fn need(self, need: &'static [u16]) -> Feature {
        Feature { need, ..self }
    }

    fn avoid(self, avoid: &'static [u16]) -> Feature {
        Feature { avoid, ..self }
    }

    fn oled_hidden(self) -> Feature {
        Feature { hide_oled: true, ..self }
    }

    fn packed(self, mask: u16) -> Feature {
        Feature { packed: true, mask, ..self }
    }

    pub fn group_rank(&self) -> usize {
        GROUPS.iter().position(|g| *g == self.group).unwrap_or(GROUPS.len())
    }

    pub fn choice_for(&self, value: u16) -> Option<&Choice> {
        self.choices.iter().find(|c| c.raw & self.mask == value & self.mask)
    }
}

fn c(raw: u16, name: &str, label: &str) -> Choice {
    Choice { raw, name: name.into(), label: label.into() }
}

fn choices(list: &[(u16, &str, &str)]) -> Vec<Choice> {
    list.iter().map(|&(raw, name, label)| c(raw, name, label)).collect()
}

fn off_on() -> Vec<Choice> {
    choices(&[(0, "off", "Off"), (1, "on", "On")])
}

/// `VCPAPI.GetSplendidModeFromTable` (low byte of 0xDC).
fn modes(line: ProductLine) -> Vec<Choice> {
    match line {
        ProductLine::Gaming => choices(&[
            (1, "cinema", "Cinema"), (2, "scenery", "Scenery"), (3, "srgb", "sRGB"), (4, "user", "User"),
            (5, "racing", "Racing"), (6, "rts", "RTS/RPG"), (7, "fps", "FPS"), (8, "moba", "MOBA"),
            (9, "night-vision", "Night Vision"), (10, "srgb-cal", "sRGB Cal"),
        ]),
        // Portable value 9 is "User" (GameVisualPage:11313); MainStream keeps Night Vision (uncertain).
        ProductLine::Portable => choices(&[
            (1, "theater", "Theater"), (2, "scenery", "Scenery"), (3, "srgb", "sRGB"), (4, "standard", "Standard"),
            (5, "game", "Game"), (6, "night", "Night View"), (7, "reading", "Reading"), (8, "darkroom", "Darkroom"),
            (9, "user", "User"),
        ]),
        _ => choices(&[
            (1, "theater", "Theater"), (2, "scenery", "Scenery"), (3, "srgb", "sRGB"), (4, "standard", "Standard"),
            (5, "game", "Game"), (6, "night", "Night View"), (7, "reading", "Reading"), (8, "darkroom", "Darkroom"),
            (9, "night-vision", "Night Vision"),
        ]),
    }
}

/// ProArt presets (VCP 0xE3). SDR presets are `preset << 8 | 0xFF`; HDR presets are the full 16-bit
/// value the app writes verbatim. HLG (0x11FF / 0x12FF) needs a two-step write, handled in `Monitor`.
fn proart_presets() -> Vec<Choice> {
    let mut v: Vec<Choice> = [
        (0x01, "native", "Native"), (0x05, "srgb", "sRGB"), (0x08, "adobe-rgb", "Adobe RGB"), (0x09, "bt2020", "BT.2020"),
        (0x0A, "dci-p3", "DCI-P3"), (0x0B, "dicom", "DICOM"), (0x0E, "rec709", "Rec.709"),
        (0x16, "user1", "User 1"), (0x17, "user2", "User 2"), (0x1E, "user3", "User 3"),
        (0x1F, "display-p3", "Display P3"), (0x20, "m-model-p3", "M Model P3"),
    ]
    .into_iter()
    .map(|(preset, name, label)| c(preset << 8 | 0xFF, name, label))
    .collect();
    v.extend(choices(&[
        (0x0F00, "hdr-pq-dci", "HDR PQ DCI"), (0x1000, "hdr-pq-bt2020", "HDR PQ BT.2020"), (0x1400, "hdr-pq", "HDR PQ"),
        (0x1303, "dolby-dark", "Dolby Vision Dark"), (0x1304, "dolby-bright", "Dolby Vision Bright"),
        (0x11FF, "hdr-hlg-bt2100", "HDR HLG BT.2100"), (0x12FF, "hdr-hlg-dci", "HDR HLG DCI"),
    ]));
    v
}

/// CapabilityConfigManager.GetColorTemperatureConfig: the same codes have different names per line.
fn color_temps(model: &str, line: ProductLine) -> Vec<Choice> {
    if line == ProductLine::ProArt {
        return choices(&[
            (0x04, "5000k", "5000K"), (0x0E, "5500k", "5500K"), (0x05, "6500k", "6500K"), (0x06, "7500k", "7500K"),
            (0x08, "9300k", "9300K"), (0x10, "12000k", "12000K"), (0x0F, "p3-theater", "P3-Theater"), (0x11, "m-model", "M Model"),
        ]);
    }
    if ["vx", "vp", "vy", "vz", "vu", "va", "vt", "be", "vg", "mb"].iter().any(|p| model.contains(p)) {
        return choices(&[(0x05, "warm", "Warm"), (0x06, "normal", "Normal"), (0x08, "cool", "Cool"), (0x0B, "user", "User")]);
    }
    choices(&[
        (0x03, "4000k", "4000K"), (0x04, "5000k", "5000K"), (0x05, "6500k", "6500K"), (0x06, "7500k", "7500K"),
        (0x07, "8200k", "8200K"), (0x08, "9300k", "9300K"), (0x09, "10000k", "10000K"), (0x0B, "user", "User"),
        // Read-only names the app also recognises (ColorPresetIndexList).
        (0x0E, "5500k", "5500K"), (0x0F, "6000k", "6000K"), (0x10, "7000k", "7000K"), (0x11, "8000k", "8000K"),
    ])
}

/// GameVisualPage HDR handlers. High byte = signal group (1 HDR10, 2 Dolby Vision). "off" is
/// read-only (the app never writes it). HDR500 (0x0108) is PG/XG/VG only; caps gating handles that.
fn hdr_modes() -> Vec<Choice> {
    choices(&[
        (0x0000, "off", "Off / no HDR signal"),
        (0x0101, "cinema", "Cinema HDR"),
        (0x0102, "gaming", "Gaming HDR"),
        (0x0103, "console", "Console HDR"),
        (0x0104, "hdr400", "DisplayHDR 400 True Black"),
        (0x0108, "hdr500", "DisplayHDR 500 True Black"),
        (0x0205, "dolby-bright", "Dolby Vision Bright"),
        (0x0206, "dolby-dark", "Dolby Vision Dark"),
        (0x0207, "dolby-gaming", "Dolby Vision Gaming"),
    ])
}

/// MCCS 0xCC OSD languages, plus ASUS additions 38 Persian and 39 Indonesian.
fn languages() -> Vec<Choice> {
    let names = [
        "zh-tw Chinese (traditional)", "en English", "fr French", "de German", "it Italian", "ja Japanese", "ko Korean",
        "pt-pt Portuguese (Portugal)", "ru Russian", "es Spanish", "sv Swedish", "tr Turkish", "zh-cn Chinese (simplified)",
        "pt-br Portuguese (Brazil)", "ar Arabic", "bg Bulgarian", "hr Croatian", "cs Czech", "da Danish", "nl Dutch",
        "et Estonian", "fi Finnish", "el Greek", "he Hebrew", "hi Hindi", "hu Hungarian", "lv Latvian", "lt Lithuanian",
        "no Norwegian", "pl Polish", "ro Romanian", "sr Serbian", "sk Slovak", "sl Slovenian", "th Thai", "uk Ukrainian",
        "vi Vietnamese", "fa Persian", "id Indonesian",
    ];
    // MCCS 0xCC numbering starts at 1.
    (1..).zip(names).map(|(raw, s)| {
        let (name, label) = s.split_once(' ').unwrap_or((s, s));
        c(raw, name, label)
    })
    .collect()
}

/// VCP 0x60 input sources (`VCPAPI.GetSourceTypeFromTable`), used for the input picker and PIP.
fn inputs() -> &'static [(u16, &'static str, &'static str)] {
    &[
        (0x01, "vga1", "VGA 1"), (0x02, "vga2", "VGA 2"), (0x03, "dvi1", "DVI 1"), (0x04, "dvi2", "DVI 2"),
        (0x05, "comp1", "Composite 1"), (0x06, "comp2", "Composite 2"), (0x07, "svideo1", "S-Video 1"), (0x08, "svideo2", "S-Video 2"),
        (0x09, "tuner1", "Tuner 1"), (0x0A, "tuner2", "Tuner 2"), (0x0B, "tuner3", "Tuner 3"),
        (0x0C, "comp-video1", "Component 1"), (0x0D, "comp-video2", "Component 2"), (0x0E, "comp-video3", "Component 3"),
        (0x0F, "dp1", "DisplayPort 1"), (0x10, "dp2", "DisplayPort 2"),
        (0x11, "hdmi1", "HDMI 1"), (0x12, "hdmi2", "HDMI 2"), (0x13, "hdmi3", "HDMI 3"), (0x14, "hdmi4", "HDMI 4"),
        (0x15, "tb1", "Thunderbolt 1"), (0x16, "tb2", "Thunderbolt 2"), (0x17, "tb3", "Thunderbolt 3"), (0x18, "tb4", "Thunderbolt 4"),
        (0x1A, "usbc1", "USB-C 1"), (0x1B, "usbc2", "USB-C 2"), (0x1C, "usbc3", "USB-C 3"), (0x1D, "usbc4", "USB-C 4"),
    ]
}

/// GamePlus crosshair styles (GamePlusPage + dictionary). 13-15 exist on some monitors but the app
/// doesn't name them.
fn crosshairs() -> Vec<Choice> {
    choices(&[
        (0, "off", "Off"),
        (1, "red-dot", "Red Dot"), (2, "green-dot", "Green Dot"),
        (3, "red-bullseye", "Red Bullseye"), (4, "green-bullseye", "Green Bullseye"),
        (5, "red-rangefinder", "Red Rangefinder"), (6, "green-rangefinder", "Green Rangefinder"),
        (7, "blue-dot", "Blue Dot"), (8, "green-dot-2", "Green Dot (2)"),
        (9, "blue-mini-duplex", "Blue Mini Duplex"), (10, "green-mini-duplex", "Green Mini Duplex"),
        (11, "blue-heavy-duplex", "Blue Heavy Duplex"), (12, "green-heavy-duplex", "Green Heavy Duplex"),
    ])
}

/// Power saving labels differ by line (CapabilityConfigManager / SystemSettingsPage).
fn power_saving(line: ProductLine) -> Vec<Choice> {
    if line == ProductLine::Gaming {
        choices(&[(0, "standard", "Standard"), (1, "power-saving", "Power Saving")])
    } else {
        choices(&[(0, "normal", "Normal Level"), (1, "deep", "Deep Level")])
    }
}

/// OSD shortcut functions per line (OSDControlManager). Value 0 = none.
fn shortcut_fns(line: ProductLine) -> Vec<(u16, &'static str, &'static str)> {
    let mut v = vec![(0u16, "none", "None")];
    v.extend(match line {
        ProductLine::ProArt => vec![
            (1, "user1", "User Mode 1"), (2, "user2", "User Mode 2"), (3, "brightness", "Brightness"),
            (4, "power-saving", "Power Saving"), (5, "motion-sync", "Motion Sync"), (6, "contrast", "Contrast"),
            (7, "input", "Input"), (8, "hdr", "HDR"), (9, "blue-light", "Blue Light Filter"), (10, "color-temp", "Color Temp"),
            (11, "volume", "Volume"), (12, "preset", "Preset"), (14, "pip", "PIP"), (15, "user3", "User Mode 3"),
        ],
        ProductLine::MainStream => vec![
            (1, "quickfit", "QuickFit"), (2, "splendid", "Splendid"), (3, "brightness", "Brightness"),
            (4, "power-saving", "Power Saving"), (5, "rest-reminder", "Rest Reminder"), (6, "contrast", "Contrast"),
            (7, "input", "Input"), (9, "blue-light", "Blue Light Filter"), (10, "color-temp", "Color Temp"),
            (11, "volume", "Volume"), (12, "color-aug", "Color Augmentation"),
        ],
        ProductLine::Portable => vec![
            (1, "auto-rotate", "Auto Rotation"), (2, "splendid", "Splendid"), (3, "brightness", "Brightness"),
            (5, "display-settings", "Display Settings"), (6, "contrast", "Contrast"), (7, "input", "Input"),
            (9, "blue-light", "Blue Light Filter"), (10, "color-temp", "Color Temp"), (11, "volume", "Volume"),
        ],
        _ => vec![
            (1, "gameplus", "GamePlus"), (2, "gamevisual", "GameVisual"), (3, "brightness", "Brightness"),
            (4, "mute", "Mute"), (5, "shadow-boost", "Shadow Boost"), (6, "contrast", "Contrast"), (7, "input", "Input"),
            (8, "hdr", "HDR"), (9, "blue-light", "Blue Light Filter"), (10, "color-temp", "Color Temp"), (11, "volume", "Volume"),
            (12, "kvm", "KVM"), (13, "smart-kvm", "Smart KVM"), (14, "frame-boost", "Frame-rate Boost"),
            (15, "pixel-cleaning", "Pixel Cleaning"),
        ],
    });
    v
}

/// Rest reminder: 0 off, then 5-60 minutes in 5-minute steps.
fn rest_reminder() -> Vec<Choice> {
    let mut v = vec![c(0, "off", "Off")];
    v.extend((1..=12u16).map(|n| {
        let min = n * 5;
        c(n, &format!("{min}min"), &format!("{min} min"))
    }));
    v
}

/// Every feature for a model. `Monitor` filters by VCP table, capabilities and vendor code.
pub fn all(model: &str) -> Vec<Feature> {
    use Table::{Legacy, New};
    let model = model.trim().to_ascii_lowercase();
    let line = ProductLine::from_model(&model);
    let proart = line == ProductLine::ProArt;
    let hdr_note = "Only applies while the monitor receives an HDR signal.";

    // Packed per-window selectors for PIP/PBP (low byte = window A, high byte = window B).
    let packed_inputs = |shift: u16| inputs().iter().map(|&(v, n, l)| c(v << shift, n, l)).collect::<Vec<_>>();
    let packed_modes = |shift: u16| modes(line).into_iter().map(|c| Choice { raw: c.raw << shift, ..c }).collect::<Vec<_>>();
    let shortcut = |shift: u16| shortcut_fns(line).into_iter().map(|(v, n, l)| c(v << shift, n, l)).collect::<Vec<_>>();

    let mut f = vec![
        // ---- Standard MCCS (work on any monitor) ----
        Feature::slider("brightness", "Brightness", 0x10, "Picture"),
        Feature::slider("contrast", "Contrast", 0x12, "Picture"),
        Feature::choose("color-temp", "Color Temperature", 0x14, "Picture", color_temps(&model, line)).mask(0x00FF),
        Feature::slider("red", "Red Gain", 0x16, "Color"),
        Feature::slider("green", "Green Gain", 0x18, "Color"),
        Feature::slider("blue", "Blue Gain", 0x1A, "Color"),
        Feature::slider("sat-red", "Saturation: Red", 0x59, "Color"),
        Feature::slider("sat-yellow", "Saturation: Yellow", 0x5A, "Color"),
        Feature::slider("sat-green", "Saturation: Green", 0x5B, "Color"),
        Feature::slider("sat-cyan", "Saturation: Cyan", 0x5C, "Color"),
        Feature::slider("sat-blue", "Saturation: Blue", 0x5D, "Color"),
        Feature::slider("sat-magenta", "Saturation: Magenta", 0x5E, "Color"),
        Feature::slider("volume", "Volume", 0x62, "System"),
        Feature::choose("input", "Input Source", 0x60, "System", choices(inputs())).mask(0x00FF),
        // Gamma high byte = (gamma - 1) x 100: 0x50 1.8 .. 0xA0 2.6; PG49WCD also lists 0x96 2.5.
        Feature::choose("gamma", "Gamma", 0x72, "Picture", choices(&[
            (0x5000, "1.8", "1.8"), (0x6400, "2.0", "2.0"), (0x7800, "2.2", "2.2"),
            (0x8C00, "2.4", "2.4"), (0x9600, "2.5", "2.5"), (0xA000, "2.6", "2.6"),
        ])).mask(0xFF00),
        Feature::slider("sharpness", "Sharpness", 0x87, "Picture"),
        Feature::slider("saturation", "Saturation", 0x8A, "Picture"),
        Feature::slider("hue", "Hue", 0x90, "Picture"),
        Feature::info("orientation", "Orientation", ORIENTATION, "System"),
        Feature::info("usage-hours", "Usage Time (hours)", 0xC0, "Info"),
        Feature::choose("language", "OSD Language", 0xCC, "System", languages()).mask(0x00FF),
        Feature::choose("power", "Power", 0xD6, "System", choices(&[(1, "on", "On"), (4, "standby", "Standby"), (5, "off", "Off")]))
            .forced("The monitor may stop answering until you press its power button."),
        Feature::info("battery", "Battery (%)", 0xD5, "System"),
        Feature::info("pd-in", "USB-C Power In", 0xD8, "System"),
        Feature::info("pd-out", "USB-C Power Out", 0xD9, "System"),

        // ---- ASUS private (gated on the 0xEF vendor code) ----
        if proart {
            Feature::choose("mode", "Preset", 0xE3, "Picture", proart_presets()).mask(0xFF00)
        } else {
            Feature::choose("mode", if line == ProductLine::Gaming { "GameVisual" } else { "Splendid" }, MODE, "Picture", modes(line)).mask(0x00FF)
        },
        Feature::slider("od", "Variable OD", 0xE0, "Gaming").oled_hidden(),
        Feature::choose("power-saving", "Power Saving", 0xE1, "System", power_saving(line)).only(New),
        Feature::choose("power-saving", "Power Saving", 0xEE, "System", power_saving(line)).only(Legacy),
        Feature::choose("ascr", "ASCR", 0xE1, "Picture", off_on()).only(Legacy).oled_hidden(),
        Feature::choose("hdr", "HDR Mode", 0xE2, "HDR", hdr_modes()).only(New).note(hdr_note),
        Feature::choose("hdr", "HDR Mode", 0xF8, "HDR", hdr_modes()).only(Legacy).note(hdr_note),
        Feature::choose("crosshair", "Crosshair", 0xE3, "GamePlus", crosshairs()),
        Feature::choose("timer", "Timer", 0xE4, "GamePlus", choices(&[
            (0, "off", "Off"), (1, "30", "30 min"), (2, "40", "40 min"), (3, "50", "50 min"), (4, "60", "60 min"), (5, "90", "90 min"),
        ])),
        Feature::choose("shadow-boost", "Shadow Boost", 0xE5, "Picture", choices(&[
            (0, "off", "Off"), (1, "level1", "Level 1"), (2, "level2", "Level 2"), (3, "level3", "Level 3"), (4, "dynamic", "Dynamic Adjustment"),
        ])),
        Feature::slider("blue-light", "Blue Light Filter", 0xE6, "Picture"),
        Feature::choose("display-alignment", "Display Alignment", 0xE7, "GamePlus", off_on()),
        Feature::action("gameplus-position", "GamePlus Position", 0xE8, "GamePlus", choices(&[
            (1, "up", "Up"), (2, "down", "Down"), (3, "right", "Right"), (4, "left", "Left"),
        ])).avoid(&[0xFE]),
        Feature::choose("fps-counter", "FPS Counter", 0xEA, "GamePlus", choices(&[(0, "off", "Off"), (1, "number", "Number"), (2, "graph", "Bar Graph")])),
        Feature::action("osd", "OSD Keys", 0xEB, "OSD", choices(&[
            (1, "show", "Show"), (0, "close", "Close"), (2, "up", "Up"), (3, "down", "Down"), (5, "left", "Left"),
            (4, "right", "Right"), (6, "enter", "Enter"), (7, "back", "Back"), (8, "input", "Input"),
            (9, "quickfit", "QuickFit"), (10, "shortcut1", "Shortcut 1"), (11, "shortcut2", "Shortcut 2"),
        ])),
        Feature::action("reset", "Reset Mode", 0xEC, "System", choices(&[(1, "reset", "Reset to defaults")]))
            .forced("Resets every setting of the current mode."),
        Feature::choose("rest-reminder", "Rest Reminder", 0xED, "System", rest_reminder()),
        Feature::choose("kvm", "KVM", 0xF3, "System", choices(&[
            (1, "smart", "Smart KVM"), (2, "usb-b", "USB-B Upstream"), (3, "usb-c", "USB-C Upstream"), (4, "auto", "Auto KVM"),
        ])).need(&[1, 2, 3, 4]),
        Feature::choose("pip-mode", "PIP/PBP Layout", 0xF4, "PIP", choices(&[
            (0, "off", "Off"),
            (1, "pip-tr", "PIP top-right"), (2, "pip-tl", "PIP top-left"), (3, "pip-br", "PIP bottom-right"), (4, "pip-bl", "PIP bottom-left"),
            (5, "pbp", "PBP 50/50"), (6, "pbp-wide-a", "PBP wide A"), (7, "pbp-wide-b", "PBP wide B"),
        ])),
        Feature::choose("pip-source-a", "PIP Window A Source", 0xF5, "PIP", packed_inputs(0)).packed(0x00FF),
        Feature::choose("pip-source-b", "PIP Window B Source", 0xF5, "PIP", packed_inputs(8)).packed(0xFF00),
        Feature::choose("pip-mode-a", "PIP Window A Mode", 0xF6, "PIP", packed_modes(0)).packed(0x00FF),
        Feature::choose("pip-mode-b", "PIP Window B Mode", 0xF6, "PIP", packed_modes(8)).packed(0xFF00),
        Feature::choose("pixel-clean-reminder", "Pixel Cleaning Reminder", 0xF8, "OLED Care", choices(&[
            (0, "off", "Off"), (2, "2h", "Every 2 hours"), (4, "4h", "Every 4 hours"), (8, "8h", "Every 8 hours"),
        ])).only(New),
        Feature::choose("screen-move", "Screen Move", 0xF9, "OLED Care", choices(&[
            (0, "off", "Off"), (1, "light", "Light"), (2, "middle", "Middle"), (3, "strong", "Strong"),
        ])).only(New),
        Feature::choose("osd-shortcut-1", "OSD Shortcut 1", 0xFB, "OSD", shortcut(8)).packed(0xFF00),
        Feature::choose("osd-shortcut-2", "OSD Shortcut 2", 0xFB, "OSD", shortcut(0)).packed(0x00FF),

        // ---- ToggleSettings2 (0xFD) bits confirmed written by the app ----
        Feature::toggle("ascr", "ASCR", 0x0002, "Picture").oled_hidden(),
        Feature::toggle("screen-saver", "Screen Dimming", 0x0008, "OLED Care"),
        Feature::toggle("pixel-cleaning", "Pixel Cleaning", 0x0010, "OLED Care")
            .forced("Starts the 6 minute pixel cleaning cycle. The screen goes dark."),
        Feature::toggle("auto-logo-brightness", "Logo Detection", 0x0020, "OLED Care"),
        Feature::toggle("uniform-brightness", "Uniform Brightness", 0x0040, "OLED Care"),
        Feature::toggle("taskbar-detection", "Taskbar Detection", 0x0800, "OLED Care"),
        Feature::toggle("boundary-detection", "Boundary Detection", 0x1000, "OLED Care"),
        Feature::toggle("outer-dimming", "Outer Dimming Control", 0x2000, "OLED Care"),
        Feature::toggle("global-dimming", "Global Dimming Control", 0x4000, "OLED Care"),

        Feature::info("mcu-firmware", "MCU Firmware", MCU_FIRMWARE, "Info"),
        Feature::info("vcp-version", "ASUS VCP Table", TABLE_VERSION, "Info"),
        Feature::info("firmware", "Firmware", FIRMWARE, "Info"),
    ];

    // ProArt-only controls (ProArt reuses E4/E5 and the GamePlus codes for other settings).
    if proart {
        f.push(Feature::choose("input-range", "Input Range", 0xE4, "Color", choices(&[
            (0, "auto", "Auto"), (1, "full", "Full"), (2, "limited-235", "Limited 16-235"), (3, "limited-254", "Limited 16-254"), (4, "sdi-full", "SDI Full"),
        ])));
        f.push(Feature::choose("backlight", "Backlight", 0x6B, "Picture", choices(&[(0, "normal", "Normal"), (1, "deep", "Deep")])));
        f.push(Feature::slider("signal", "Black Level Signal", 0xF2, "Picture"));
        f.push(Feature::slider("offset-red", "Red Offset", 0x6C, "Color"));
        f.push(Feature::slider("offset-green", "Green Offset", 0x6E, "Color"));
        f.push(Feature::slider("offset-blue", "Blue Offset", 0x70, "Color"));
    }

    f.into_iter()
        // ProArt reuses the GamePlus codes (0xE3-0xEA), so those features don't apply there.
        .filter(|f| !(proart && f.group == "GamePlus"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_lines_match_the_original_app() {
        for (model, line) in [
            ("XG27ACDMS", ProductLine::Gaming),
            ("VG27AQL1A", ProductLine::Gaming),
            ("PA32UCXR", ProductLine::ProArt),
            ("VA24EHE", ProductLine::MainStream),
            ("MB16AHG", ProductLine::Portable),
            ("Dell", ProductLine::Unknown),
        ] {
            assert_eq!(ProductLine::from_model(model), line, "{model}");
        }
    }

    #[test]
    fn oled_detection() {
        assert!(is_oled("XG27ACDMS"), "index 6 is 'd'");
        assert!(is_oled("PG27AQDM"));
        assert!(is_oled("MQ149CD"));
        assert!(!is_oled("VG27AQL1A"), "index 6 is 'l'");
        assert!(!is_oled("PA32UCXR"));
    }

    #[test]
    fn vendor_codes() {
        assert!(is_asus_vendor(0xB1)); // XG27ACDMS
        assert!(is_asus_vendor(0xA0)); // VG27AQL1A
        assert!(!is_asus_vendor(1));
        assert!(!is_asus_vendor(0));
        assert!(is_private(0xDC) && is_private(0xE2) && is_private(0xFD));
        assert!(!is_private(0x10) && !is_private(0xFF) && !is_private(0xCC));
    }

    #[test]
    fn line_specific_choices() {
        let find = |model: &str, key: &str| all(model).into_iter().find(|f| f.key == key).unwrap();
        let name = |f: &Feature, raw| f.choice_for(raw).map(|c| c.name.clone());

        assert_eq!(name(&find("VG27AQL1A", "color-temp"), 0x05).as_deref(), Some("warm"));
        assert_eq!(name(&find("XG27ACDMS", "color-temp"), 0x05).as_deref(), Some("6500k"));

        let preset = find("PA32UCXR", "mode");
        assert_eq!(preset.code, 0xE3);
        assert_eq!(name(&preset, 0x05FF).as_deref(), Some("srgb"));
        assert!(!all("PA32UCXR").iter().any(|f| f.key == "crosshair"), "0xE3 is the preset on ProArt");
        assert!(all("PA32UCXR").iter().any(|f| f.key == "input-range"), "E4 is input range on ProArt");
    }

    #[test]
    fn packed_pip_source_positions() {
        let a = all("XG27ACDMS").into_iter().find(|f| f.key == "pip-source-a").unwrap();
        let b = all("XG27ACDMS").into_iter().find(|f| f.key == "pip-source-b").unwrap();
        assert_eq!(a.choice_for(0x0F).unwrap().name, "dp1"); // low byte
        assert_eq!(b.choice_for(0x0F00).unwrap().name, "dp1"); // high byte
    }

    #[test]
    fn keys_are_unique_per_table() {
        for table in [Table::New, Table::Legacy] {
            let mut keys: Vec<_> = all("XG27ACDMS")
                .into_iter()
                .filter(|f| f.table == Table::Any || f.table == table)
                .map(|f| f.key)
                .collect();
            let count = keys.len();
            keys.sort_unstable();
            keys.dedup();
            assert_eq!(keys.len(), count, "{table:?}");
        }
    }
}
