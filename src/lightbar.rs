//! ASUS monitor light bar over USB HID (`0b05:1ac8`), reverse engineered from `ScreenLightBarHid.dll`.
//!
//! Every transfer is a 65-byte report whose first byte is report id `0xEC`. White light is two 0-255
//! duty values (warm + cool) that sum to the brightness; RGB uses the effect commands. Untested
//! without the hardware. Linux only for now (hidraw); Windows returns "not implemented".

pub const VID: u16 = 0x0B05;
pub const PID: u16 = 0x1AC8;

#[cfg(any(target_os = "linux", test))]
const REPORT_ID: u8 = 0xEC;
#[cfg(any(target_os = "linux", test))]
const REPORT_LEN: usize = 65;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Static = 1,
    Breath = 2,
    Strobe = 3,
    Cycle = 4,
    Rainbow = 5,
}

/// Splits a 0-100 brightness and 2700-6500 K temperature into cool/warm duty bytes
/// (`SLB_ComputeCtValues`): the app caps brightness at 70 % of the slider.
#[cfg(any(target_os = "linux", test))]
fn white_duty(brightness: u8, kelvin: u16) -> (u8, u8) {
    let kelvin = kelvin.clamp(2700, 6500);
    let total = (f64::from(brightness.min(100)) * 0.7 / 100.0 * 255.0).round() as u16;
    let cool = (f64::from(total) * f64::from(kelvin - 2700) / 3800.0).round() as u16;
    (cool as u8, (total - cool) as u8)
}

/// Report bytes (after the id) for a static RGB colour.
#[cfg(any(target_os = "linux", test))]
fn rgb_report(effect: Effect, r: u8, g: u8, b: u8) -> [u8; REPORT_LEN] {
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = REPORT_ID;
    buf[1] = 0x35;
    buf[5] = effect as u8;
    buf[8] = 0x01;
    buf[9] = r;
    buf[10] = g;
    buf[11] = b;
    buf
}

/// Report bytes for white light from cool/warm duty values.
#[cfg(any(target_os = "linux", test))]
fn white_report(cool: u8, warm: u8) -> [u8; REPORT_LEN] {
    let mut buf = [0u8; REPORT_LEN];
    buf[0] = REPORT_ID;
    buf[1] = 0x35;
    buf[2] = 0x01;
    buf[5] = 0x01;
    buf[8] = 0x01;
    buf[9] = cool;
    buf[10] = warm;
    buf
}

#[cfg(target_os = "linux")]
pub use linux::{LightBar, find};

#[cfg(target_os = "linux")]
mod linux {
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;

    use super::{Effect, PID, REPORT_LEN, VID, rgb_report, white_duty, white_report};

    pub struct LightBar {
        path: String,
        file: File,
    }

    /// Every light bar on the system (matched by VID/PID through `/sys/class/hidraw`).
    pub fn find() -> Vec<LightBar> {
        let Ok(entries) = fs::read_dir("/sys/class/hidraw") else {
            return Vec::new();
        };
        let want = format!("{VID:04X}:{PID:04X}");
        let mut bars = Vec::new();
        for entry in entries.flatten() {
            let uevent = entry.path().join("device/uevent");
            let Ok(text) = fs::read_to_string(&uevent) else { continue };
            // HID_ID=0003:00000B05:00001AC8
            if !text.lines().any(|l| l.strip_prefix("HID_ID=").is_some_and(|id| id.to_uppercase().contains(&want))) {
                continue;
            }
            let dev = PathBuf::from("/dev").join(entry.file_name());
            if let Ok(file) = OpenOptions::new().read(true).write(true).open(&dev) {
                bars.push(LightBar { path: dev.to_string_lossy().into_owned(), file });
            }
        }
        bars
    }

    impl LightBar {
        pub fn path(&self) -> &str {
            &self.path
        }

        fn send(&mut self, report: [u8; REPORT_LEN]) -> Result<(), String> {
            self.file.write_all(&report).map_err(|e| format!("{}: {e}", self.path))
        }

        /// White light at a 0-100 brightness and 2700-6500 K temperature.
        pub fn set_white(&mut self, brightness: u8, kelvin: u16) -> Result<(), String> {
            let (cool, warm) = white_duty(brightness, kelvin);
            self.send(white_report(cool, warm))
        }

        pub fn set_rgb(&mut self, effect: Effect, r: u8, g: u8, b: u8) -> Result<(), String> {
            self.send(rgb_report(effect, r, g, b))
        }

        pub fn off(&mut self) -> Result<(), String> {
            self.send(rgb_report(Effect::Static, 0, 0, 0))
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub struct LightBar;

#[cfg(not(target_os = "linux"))]
pub fn find() -> Vec<LightBar> {
    Vec::new()
}

#[cfg(not(target_os = "linux"))]
impl LightBar {
    pub fn path(&self) -> &str {
        ""
    }
    pub fn set_white(&mut self, _brightness: u8, _kelvin: u16) -> Result<(), String> {
        Err("light bar control is Linux-only for now".into())
    }
    pub fn set_rgb(&mut self, _effect: Effect, _r: u8, _g: u8, _b: u8) -> Result<(), String> {
        Err("light bar control is Linux-only for now".into())
    }
    pub fn off(&mut self) -> Result<(), String> {
        Err("light bar control is Linux-only for now".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_duty_splits_and_caps() {
        // Full brightness caps at 70 % -> total 179; pure cool at 6500 K, pure warm at 2700 K.
        assert_eq!(white_duty(100, 6500), (179, 0));
        assert_eq!(white_duty(100, 2700), (0, 179));
        assert_eq!(white_duty(0, 5000), (0, 0));
        // Mid temperature splits, and the two always sum to the total.
        let (c, w) = white_duty(100, 4600);
        assert_eq!(u16::from(c) + u16::from(w), 179);
    }

    #[test]
    fn report_framing() {
        let r = rgb_report(Effect::Breath, 10, 20, 30);
        assert_eq!(r[0], REPORT_ID);
        assert_eq!((r[1], r[5], r[8]), (0x35, 2, 1));
        assert_eq!(&r[9..12], &[10, 20, 30]);
        let w = white_report(100, 50);
        assert_eq!((w[2], w[9], w[10]), (1, 100, 50));
    }
}
