//! The feature map, reverse engineered from ASUS DisplayWidget Center 1.3.0.4
//! (VCPAPI.cs, AirVisionVCPCode.cs, VCPColorTemperature.cs and the GameVisual/OLED/GamePlus pages).
//! docs/PROTOCOL.md has the evidence for each entry.

use serde::Serialize;

pub const MODE: u8 = 0xDC;
pub const TABLE_VERSION: u8 = 0xEF;
pub const FIRMWARE: u8 = 0xFF;
pub const TOGGLES: u8 = 0xFD;

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
    /// Disruptive: needs `force`.
    pub force: bool,
    pub note: Option<&'static str>,
}

impl Feature {
    fn new(key: &'static str, label: &'static str, code: u8, kind: Kind, group: &'static str) -> Feature {
        Feature { key, label, code, kind, group, choices: Vec::new(), mask: 0xFFFF, bit: 0, table: Table::Any, force: false, note: None }
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

/// `off` for 0, then `{prefix}{n}` up to `last`.
fn levels(first: u16, last: u16, prefix: &str) -> Vec<Choice> {
    (first..=last)
        .map(|n| if n == 0 { c(0, "off", "Off") } else { c(n, &format!("{prefix}{n}"), &format!("{prefix}{n}")) })
        .collect()
}

fn off_on() -> Vec<Choice> {
    choices(&[(0, "off", "Off"), (1, "on", "On")])
}

/// `VCPAPI.GetSplendidModeFromTable` (low byte of 0xDC).
fn modes(line: ProductLine) -> Vec<Choice> {
    match line {
        ProductLine::Gaming => choices(&[
            (1, "cinema", "Cinema"),
            (2, "scenery", "Scenery"),
            (3, "srgb", "sRGB"),
            (4, "user", "User"),
            (5, "racing", "Racing"),
            (6, "rts", "RTS/RPG"),
            (7, "fps", "FPS"),
            (8, "moba", "MOBA"),
            (9, "night-vision", "Night Vision"),
            (10, "srgb-cal", "sRGB Cal"),
        ]),
        _ => choices(&[
            (1, "theater", "Theater"),
            (2, "scenery", "Scenery"),
            (3, "srgb", "sRGB"),
            (4, "standard", "Standard"),
            (5, "game", "Game"),
            (6, "night", "Night View"),
            (7, "reading", "Reading"),
            (8, "darkroom", "Darkroom"),
            (9, "night-vision", "Night Vision"),
        ]),
    }
}

/// ProArt presets live in 0xE3 as `preset << 8 | 0xFF` (VCPAPI.SetDisplayMode).
fn proart_presets() -> Vec<Choice> {
    [
        (0x01, "native", "Native"), (0x05, "srgb", "sRGB"), (0x08, "adobe-rgb", "Adobe RGB"), (0x09, "rec2020", "Rec.2020"),
        (0x0A, "dci-p3", "DCI-P3"), (0x0B, "dicom", "DICOM"), (0x0E, "rec709", "Rec.709"), (0x0F, "hdr-pq-dci", "HDR PQ DCI"),
        (0x10, "hdr-pq-rec2020", "HDR PQ Rec.2020"), (0x11, "hdr-hlg-bt2100", "HDR HLG BT.2100"), (0x12, "hdr-hlg-dci", "HDR HLG DCI"),
        (0x13, "dolby-vision", "Dolby Vision"), (0x14, "hdr", "HDR"), (0x16, "user1", "User 1"), (0x17, "user2", "User 2"),
        (0x1E, "user3", "User 3"), (0x1F, "display-p3", "Display P3"), (0x20, "m-model-p3", "M Model P3"),
    ]
    .into_iter()
    .map(|(preset, name, label)| c(preset << 8 | 0xFF, name, label))
    .collect()
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
    ])
}

/// GameVisualPage HDR handlers (HDRCinema_Checked ... DolbySoftware_Checked).
fn hdr_modes() -> Vec<Choice> {
    choices(&[
        (0x0000, "off", "Off (no HDR signal)"),
        (0x0101, "cinema", "Cinema HDR"),
        (0x0102, "gaming", "Gaming HDR"),
        (0x0103, "console", "Console HDR"),
        (0x0104, "hdr400", "DisplayHDR 400 True Black"),
        (0x0108, "hdr500", "DisplayHDR 500 True Black"),
        (0x0205, "dolby-bright", "Dolby Vision Bright"),
        (0x0206, "dolby-dark", "Dolby Vision Dark"),
        (0x0207, "dolby-gaming", "Dolby Vision Gaming"),
        (0x0208, "dolby-source", "Dolby Vision (Source-Only)"),
    ])
}

fn languages() -> Vec<Choice> {
    let names = [
        "zh-tw Chinese (traditional)", "en English", "fr French", "de German", "it Italian", "ja Japanese", "ko Korean",
        "pt-pt Portuguese (Portugal)", "ru Russian", "es Spanish", "sv Swedish", "tr Turkish", "zh-cn Chinese (simplified)",
        "pt-br Portuguese (Brazil)", "ar Arabic", "bg Bulgarian", "hr Croatian", "cs Czech", "da Danish", "nl Dutch",
        "et Estonian", "fi Finnish", "el Greek", "he Hebrew", "hi Hindi", "hu Hungarian", "lv Latvian", "lt Lithuanian",
        "no Norwegian", "pl Polish", "ro Romanian", "sr Serbian", "sk Slovak", "sl Slovenian", "th Thai", "uk Ukrainian",
        "vi Vietnamese",
    ];
    // MCCS 0xCC numbering starts at 1.
    (1..).zip(names).map(|(raw, s)| {
        let (name, label) = s.split_once(' ').unwrap_or((s, s));
        c(raw, name, label)
    })
    .collect()
}

/// Every feature for a model. `Monitor` filters by VCP table and capabilities.
pub fn all(model: &str) -> Vec<Feature> {
    use Table::{Legacy, New};
    let model = model.trim().to_ascii_lowercase();
    let line = ProductLine::from_model(&model);
    let proart = line == ProductLine::ProArt;
    let saving = || choices(&[(0, "standard", "Standard"), (1, "power-saving", "Power Saving")]);
    let hdr_note = "Only applies while the monitor receives an HDR signal.";

    vec![
        // Standard MCCS
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
        Feature::choose("input", "Input Source", 0x60, "System", choices(&[
            (0x01, "vga1", "VGA 1"), (0x03, "dvi1", "DVI 1"), (0x04, "dvi2", "DVI 2"),
            (0x0F, "dp1", "DisplayPort 1"), (0x10, "dp2", "DisplayPort 2"),
            (0x11, "hdmi1", "HDMI 1"), (0x12, "hdmi2", "HDMI 2"), (0x13, "hdmi3", "HDMI 3"), (0x14, "hdmi4", "HDMI 4"),
            (0x15, "tb1", "Thunderbolt 1"), (0x16, "tb2", "Thunderbolt 2"),
            (0x1A, "usbc1", "USB-C 1"), (0x1B, "usbc2", "USB-C 2"),
        ])).mask(0x00FF),
        // High byte = gamma x 100 - 100 (GameVisualPage.LoadGamma).
        Feature::choose("gamma", "Gamma", 0x72, "Picture", choices(&[
            (0x5000, "1.8", "1.8"), (0x6400, "2.0", "2.0"), (0x7800, "2.2", "2.2"), (0x8C00, "2.4", "2.4"), (0xA000, "2.6", "2.6"),
        ])).mask(0xFF00),
        Feature::slider("sharpness", "Sharpness", 0x87, "Picture"),
        Feature::slider("saturation", "Saturation", 0x8A, "Picture"),
        Feature::slider("hue", "Skin Tone", 0x90, "Picture"),
        Feature::new("usage-hours", "Usage Time (hours)", 0xC0, Kind::Info, "Info"),
        Feature::choose("language", "OSD Language", 0xCC, "System", languages()).mask(0x00FF),
        Feature::choose("power", "Power", 0xD6, "System", choices(&[(1, "on", "On"), (4, "standby", "Standby"), (5, "off", "Off")]))
            .forced("The monitor may stop answering until you press its power button."),
        // ASUS private codes
        if proart {
            Feature::choose("mode", "Preset", 0xE3, "Picture", proart_presets()).mask(0xFF00)
        } else {
            Feature::choose("mode", if line == ProductLine::Gaming { "GameVisual" } else { "Splendid" }, MODE, "Picture", modes(line))
                .mask(0x00FF)
        },
        Feature::slider("od", "Variable OD", 0xE0, "Gaming"),
        Feature::choose("power-saving", "Power Saving", 0xE1, "System", saving()).only(New),
        Feature::choose("power-saving", "Power Saving", 0xEE, "System", saving()).only(Legacy),
        Feature::choose("ascr", "ASCR", 0xE1, "Picture", off_on()).only(Legacy),
        Feature::choose("hdr", "HDR Mode", 0xE2, "HDR", hdr_modes()).only(New).note(hdr_note),
        Feature::choose("hdr", "HDR Mode", 0xF8, "HDR", hdr_modes()).only(Legacy).note(hdr_note),
        Feature::choose("crosshair", "Crosshair", 0xE3, "GamePlus", levels(0, 15, "style")),
        // CapabilityConfigManager.GetGamePlusTimerConfig
        Feature::choose("timer", "Timer", 0xE4, "GamePlus", choices(&[
            (0, "off", "Off"), (1, "30", "30 min"), (2, "40", "40 min"), (3, "50", "50 min"), (4, "60", "60 min"), (5, "90", "90 min"),
        ])),
        Feature::choose("shadow-boost", if proart { "Dynamic Dimming" } else { "Shadow Boost" }, 0xE5, "Picture", levels(0, 4, "level")),
        Feature::slider("blue-light", "Blue Light Filter", 0xE6, "Picture"),
        Feature::choose("quickfit", "QuickFit", 0xE7, "GamePlus", off_on()),
        Feature::choose("gameplus-position", "GamePlus Position", 0xE8, "GamePlus", levels(1, 8, "pos")),
        Feature::choose("fps-counter", "FPS Counter", 0xEA, "GamePlus", choices(&[(0, "off", "Off"), (1, "number", "Number"), (2, "graph", "Graph")])),
        Feature::action("osd", "OSD Keys", 0xEB, "OSD", choices(&[
            (1, "show", "Show"), (0, "close", "Close"), (2, "up", "Up"), (3, "down", "Down"), (5, "left", "Left"),
            (4, "right", "Right"), (6, "enter", "Enter"), (7, "back", "Back"), (8, "input", "Input"),
            (9, "quickfit", "QuickFit"), (10, "shortcut1", "Shortcut 1"), (11, "shortcut2", "Shortcut 2"),
        ])),
        Feature::action("reset", "Reset Mode", 0xEC, "System", choices(&[(1, "reset", "Reset to defaults")]))
            .forced("Resets every setting of the current mode."),
        Feature::choose("kvm", "KVM", 0xF3, "System", { let mut k = levels(1, 4, "kvm"); k.push(c(0xFE, "n/a", "Not available")); k }),
        Feature::choose("pip-mode", "PIP/PBP Mode", 0xF4, "PIP", levels(0, 5, "mode")),
        Feature::slider("pip-source", "PIP/PBP Source", 0xF5, "PIP"),
        Feature::slider("pip-color", "PIP/PBP Color", 0xF6, "PIP"),
        // CapabilityConfigManager.GetPixelCleaningReminderConfig: hours.
        Feature::choose("pixel-clean-reminder", "Pixel Cleaning Reminder", 0xF8, "OLED Care", choices(&[
            (0, "off", "Off"), (2, "2h", "Every 2 hours"), (4, "4h", "Every 4 hours"), (8, "8h", "Every 8 hours"),
        ])).only(New),
        Feature::choose("screen-move", "Screen Move", 0xF9, "OLED Care", choices(&[
            (0, "off", "Off"), (1, "light", "Light"), (2, "middle", "Middle"), (3, "strong", "Strong"),
        ])).only(New),
        // VCPAPI.ToggleSettings2; ProArt reuses some bits.
        Feature::toggle("vrr", if proart { "HDR Preview" } else { "Adaptive-Sync (VRR)" }, 0x0001, "Gaming"),
        Feature::toggle("ascr", if proart { "Dolby Vision" } else { "ASCR" }, 0x0002, "Picture"),
        Feature::toggle("sniper", if proart { "Preset Lock" } else { "Sniper" }, 0x0004, if proart { "System" } else { "GamePlus" }),
        Feature::toggle("screen-saver", "Screen Saver", 0x0008, "OLED Care"),
        Feature::toggle("pixel-cleaning", "Pixel Cleaning", 0x0010, "OLED Care")
            .forced("Starts the 6 minute pixel cleaning cycle. The screen goes dark."),
        Feature::toggle("auto-logo-brightness", "Auto Logo Brightness", 0x0020, "OLED Care"),
        Feature::toggle("uniform-brightness", "Uniform Brightness", 0x0040, "OLED Care"),
        Feature::toggle("screen-protection", "Screen Protection", 0x0080, "OLED Care"),
        Feature::toggle("uniformity-compensation", "Uniformity Compensation", 0x0100, "Picture"),
        Feature::toggle("ai-gamevisual", "AI GameVisual", 0x0200, "Gaming"),
        Feature::toggle("inner-color-temp", "Inner Color Temperature", 0x0400, "Picture"),
        Feature::toggle("taskbar-detection", "Taskbar Detection", 0x0800, "OLED Care"),
        Feature::toggle("boundary-detection", "Boundary Detection", 0x1000, "OLED Care"),
        Feature::toggle("outer-dimming", "Outer Dimming Control", 0x2000, "OLED Care"),
        Feature::toggle("global-dimming", "Global Dimming Control", 0x4000, "OLED Care"),
        Feature::new("vcp-version", "ASUS VCP Table", TABLE_VERSION, Kind::Info, "Info"),
        Feature::new("firmware", "Firmware", FIRMWARE, Kind::Info, "Info"),
    ]
    .into_iter()
    // ProArt reuses the GamePlus codes (0xE3-0xEA) for its own settings.
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
    fn line_specific_choices() {
        let find = |model: &str, key: &str| all(model).into_iter().find(|f| f.key == key).unwrap();
        let name = |f: &Feature, raw| f.choice_for(raw).map(|c| c.name.clone());

        assert_eq!(name(&find("VG27AQL1A", "color-temp"), 0x05).as_deref(), Some("warm"));
        assert_eq!(name(&find("XG27ACDMS", "color-temp"), 0x05).as_deref(), Some("6500k"));

        let preset = find("PA32UCXR", "mode");
        assert_eq!(preset.code, 0xE3);
        assert_eq!(name(&preset, 0x05FF).as_deref(), Some("srgb"));
        assert!(!all("PA32UCXR").iter().any(|f| f.key == "crosshair"), "0xE3 is the preset on ProArt");
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
