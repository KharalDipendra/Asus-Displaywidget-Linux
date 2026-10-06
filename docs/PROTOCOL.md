# ASUS DisplayWidget Center — reverse-engineered monitor protocol

Source: ASUS DisplayWidget Center **1.3.0.4** (`C:\Program Files (x86)\ASUS\DisplayWidgetCenter`),
decompiled with ILSpy (`ilspycmd`). The app is .NET (WPF) — it decompiles to readable C#.
The decompiled sources are not part of this repository (ASUS copyright).

Verified against real monitors on 2026-10-06, through dxva2 (the API the app itself uses):
- **XG27ACDMS** (QD-OLED, FW 102, new VCP table v2.23)
- **VG27AQL1A** (IPS, legacy VCP table)

## 1. Transport: plain DDC/CI, nothing proprietary

`DDCMethods.cs` P/Invokes only the standard Windows monitor-configuration API:

| Windows API (dxva2.dll) | Purpose |
|---|---|
| `GetPhysicalMonitorsFromHMONITOR` | handle per monitor |
| `GetVCPFeatureAndVCPFeatureReply` | read VCP code -> (current, max) |
| `SetVCPFeature` | write VCP code |
| `CapabilitiesRequestAndCapabilitiesReply` | MCCS capabilities string |

That is exactly DDC/CI over I²C (address 0x37). On Linux the same bytes go through `/dev/i2c-N`
(what `ddcutil` does). **No driver, HID, USB or network protocol is needed for monitor settings.**

Things in the app that are *not* DDC/CI (not ported, not needed for picture settings):
`HidLibrary.dll` / `ScreenLightBarHid.dll` (ASUS screen light bar), `AIEngine/` (515 MB of PyTorch
for "AI GameVisual" scene detection, which just calls `SetVCPFeature(0xDC, mode)`), firmware update,
cloud telemetry (`AsusAPI`, `EncryptedLogs.log`), AirVision glasses (TCP/IP + VCP dock protocol).

`VCPAPI.SetVCPFeatureInternal` sleeps 20 ms before each call and serialises all calls with a lock.
The gap matters: the VG27AQL1A fails back-to-back requests, so asusdisplay keeps the DDC/CI spec's
50 ms between commands.

## 2. Conventions

- **VCP table version** — `VCPAPI.GetVCPVersion`: read VCP **0xEF**; `version = (max >> 8) & 0xFF`.
  `version == 2` => "IsNewVCP". New-table monitors moved several features (see table, "gen" column).
- **Firmware** — VCP **0xFF**: `max` = firmware version, `current & 0xFF` = panel id.
  (XG27ACDMS: max = 102, matches `UserConfig/CapabilityData.json`.)
- **Unavailable** — a reply of **254 / 254** means "control locked in the current mode";
  the app greys the control out (`if (currentLocal == 254 || maxLocal == 254)`).
  Seen live: Blue Light Filter (0xE6) in User mode on XG27ACDMS.
- **Toggle registers** — **0xFD** (`ToggleSettings2`) and **0xFC** (`ToggleSettings1`) are 16-bit bit-fields.
  The app does read-modify-write (`VCPAPI.SetToggleSettingsItem`): read, `|= bit` or `&= ~bit`, write.
- **Capabilities bitmask** — `FD(7879)` in the capabilities string is the mask of implemented toggle bits:
  `0x7879 = VRR | DisplaySaver | PixelCleaning | AdjustLogo | UniformBrightness | Taskbar | Boundary | OuterDimming | GlobalDimming`
  which matches exactly the OLED Care page of the app for that model.
- **Where per-model support comes from** — not from a model list. `CapabilityConfigManager` derives every
  control from the monitor's own capabilities string (and caches it in `UserConfig/CapabilityData.json`).
  For monitors that don't answer the capabilities request, `DisplayManager.GetCapability` falls back to
  strings built into the app: PG49WCD, PG34WCDM, PG32UCDM, XG27ACDNG, and two ProArt defaults (model
  name with and without a "V"). asusdisplay ships the same fallbacks (`src/caps.rs`).
- **Product line** — `Utils.GetProductLine(model)` by name prefix:
  `XG/PG/VG` (or contains `_D`) = Gaming, `PA/PQ` = ProArt, `MB/MQ` = Portable,
  `VA/VX/VP/VY/VZ/VU/VT/BE` = MainStream. Picture-mode numbers depend on the line.

## 3. VCP code map

Standard MCCS codes used by the app:

| Code | Feature | Notes |
|---|---|---|
| 0x10 | Brightness | continuous |
| 0x12 | Contrast | continuous |
| 0x14 | Color temperature | Names depend on the model (`GetColorTemperatureConfig`). Most gaming: 3=4000K 4=5000K 5=6500K 6=7500K 7=8200K 8=9300K 9=10000K 0B=User. VG, mainstream, portable: 5=Warm 6=Normal 8=Cool 0B=User. ProArt: 4=5000K 0E=5500K 5=6500K 6=7500K 8=9300K 10=12000K 0F=P3-Theater 11=M Model |
| 0x16 / 0x18 / 0x1A | Red / Green / Blue gain | continuous |
| 0x59–0x5E | Six-axis saturation R/Y/G/C/B/M | continuous |
| 0x5F | HDR adjustment | `AirVisionVCPCode.HDRAdjustment`, semantics unknown |
| 0x60 | Input | 0x0F DP1, 0x10 DP2, 0x11 HDMI1, 0x12 HDMI2, 0x1A USB-C1, 0x1B USB-C2 (`GetSourceTypeFromTable`) |
| 0x61 | DisplayPort stream | `AirVisionVCPCode.DisplayPortStream`, unknown |
| 0x6B | Backlight | ProArt |
| 0x6C / 0x6E / 0x70 | Black level R/G/B | ProArt |
| 0x72 | Gamma | `(value & 0xFF00) >> 8` ∈ {80,100,120,140,160} = 1.8/2.0/2.2/2.4/2.6. Live XG27ACDMS read: 0x7800 = 2.2 ✔ |
| 0x87 | Sharpness | continuous |
| 0x8A | Saturation | continuous |
| 0x90 | Hue / skin tone | continuous |
| 0xAA | Orientation | auto-rotate |
| 0xC0 | Display usage time (hours) | read-only |
| 0xCC | OSD language | MCCS language codes |
| 0xD5 | Battery level | ZenScreen |
| 0xD6 | Power mode | 1 on, 5 off |
| 0xD8 / 0xD9 | USB-PD input / output | ZenScreen |

ASUS private codes (`VCPAPI.cs` + `AirVisionVCPCode.cs` enum names):

| Code | Name in app | gen | Values |
|---|---|---|---|
| 0xDC | DisplayApplication (GameVisual / Splendid) | any | see §4 |
| 0xE0 | TraceFree / Variable OD | any | index 0..max (shown x20 on legacy/MainStream/ProArt); hidden on OLED |
| 0xE1 | PowerSaving | **new** | 0 Standard, 1 Power Saving |
| 0xE1 | ASCR | legacy | 0/1 |
| 0xE2 | HDRSettings | **new** | 0 none (SDR), 0x0101 Cinema, 0x0102 Gaming, 0x0103 Console, 0x0104 HDR400 True Black, 0x0108 HDR500 True Black, 0x0205–0x0208 Dolby Vision Bright/Dark/Gaming/Source-only. Capability strings list the low byte. XG27ACDMS max = 0x0104 |
| 0xE3 | Crosshair (Gaming) / Preset (ProArt) | any | 0 off, styles 1–15 as listed by the monitor |
| 0xE4 | GamePlus timer (Gaming) / Input range (ProArt) | any | 0 off, 1–5 = 30, 40, 50, 60, 90 minutes |
| 0xE5 | Shadow Boost (Gaming) / Dynamic dimming (ProArt) | any | 0 off, 1–4 |
| 0xE6 | Blue Light Filter | any | 0–4 |
| 0xE7 | QuickFit / display alignment | any | 0/1 |
| 0xE8 | GamePlus icon position | any | **write-only nudge**: 1 Up, 2 Down, 3 Right, 4 Left (hidden if caps list 0xFE) |
| 0xE9 | OSDSettings | any | enum name only; the app never reads or writes it |
| 0xEA | FPS counter | any | 0 off, 1, 2 |
| 0xEB | OSD instruction (virtual OSD keys) | any | 0 Close, 1 Show, 2 Up, 3 Down, 4 Right, 5 Left, 6 Enter, 7 Back, 8 InputSelect, 9 QuickFit, 10 Shortcut1, 11 Shortcut2, 12 SelfCalibration |
| 0xEC | RestoreModeDefaults | any | 1 = reset current mode |
| 0xED | Rest reminder / proximity sensor | any | |
| 0xEE | PowerSaving (legacy) / ELMB | legacy | |
| 0xEF | ASUSMonitorSupport / VCP version | any | read-only, see §2 |
| 0xF2 | Aura Sync / Black level signal / Dual display | any | model dependent |
| 0xF3 | KVM | any | 1–4; 0xFE = not available |
| 0xF4 / 0xF5 / 0xF6 | PIP/PBP mode / source / color | any | |
| 0xF8 | OLED warning timer (pixel-cleaning reminder) | **new** | 0 off, 2 / 4 / 8 hours |
| 0xF8 | HDR (legacy) | legacy | |
| 0xF9 | OLED Screen Move | **new** | 0 Off, 1 Light, 2 Middle, 3 Strong |
| 0xFA | CustomizedSetting | any | unknown |
| 0xFB | Shortcuts | any | |
| 0xFC | ToggleSettings1 | any | bit-field, semantics unknown; caps `FC(0C1F)` |
| 0xFD | ToggleSettings2 | new | bit-field, §5 |
| 0xFF | FW version | any | read-only, §2 |

ProArt monitors put their preset in **0xE3** as `preset << 8 | 0xFF` (`VCPAPI.SetDisplayMode`); the
capabilities string lists the preset numbers: 01 Native, 05 sRGB, 08 Adobe RGB, 09 Rec.2020, 0A DCI-P3,
0B DICOM, 0E Rec.709, 0F HDR PQ DCI, 10 HDR PQ Rec.2020, 11 HDR HLG BT.2100, 12 HDR HLG DCI,
13 Dolby Vision, 14 HDR, 16/17/1E User 1-3, 1F Display P3, 20 M Model P3. HDR sub-modes use full 16-bit
values (0x0F00-0x1402) and are not mapped. ProArt reuses the GamePlus codes, so those are hidden there.

## 4. Picture modes (VCP 0xDC, low byte)

| Value | Gaming (GameVisual) | MainStream / Portable (Splendid) |
|---|---|---|
| 1 | Cinema | Theater |
| 2 | Scenery | Scenery |
| 3 | sRGB | sRGB |
| 4 | User | Standard |
| 5 | Racing | Game |
| 6 | RTS/RPG | Night View |
| 7 | FPS | Reading |
| 8 | MOBA | Darkroom |
| 9 | Night Vision | Night Vision |
| 10 | sRGB Cal | — |

Gaming monitors with factory sRGB calibration use 10 instead of 3 (`SetDisplayMode`); XG27ACDMS caps
list `0A` and not `03`. Caps also list `0000 0100 0200` for DC (high-byte values; meaning unknown).

## 5. ToggleSettings2 (VCP 0xFD) bits

From `VCPAPI.ToggleSettings2` (several bits are reused per product line):

| Bit | Gaming / OLED | ProArt / other |
|---|---|---|
| 0x0001 | VRR (Adaptive-Sync) | HDR Preview |
| 0x0002 | ASCR (new table) | Dolby Vision |
| 0x0004 | Sniper | Preset Lock / Export photos |
| 0x0008 | OLED Screen Saver ("DisplaySaver") | Screen center |
| 0x0010 | OLED Pixel Cleaning (starts ~6 min cycle) | Instant transparent |
| 0x0020 | Auto Logo Brightness | Brightness boost / Auto transparency |
| 0x0040 | Uniform Brightness | Overclocking |
| 0x0080 | Clear Pixel / Screen Protection | Stabilizer / Sensor calibration |
| 0x0100 | Uniformity Compensation | Spatial anchor |
| 0x0200 | AI GameVisual / MOBA map helper | Inner brightness / Increase brightness |
| 0x0400 | Inner color temperature | Spatial lock |
| 0x0800 | Taskbar Detection / Frame-rate boost | Ambient brightness |
| 0x1000 | Boundary Detection | Ambient color temperature |
| 0x2000 | Outer Dimming Control | |
| 0x4000 | Global Dimming Control | |

Live XG27ACDMS value: `0x7869` (VRR, Screen Saver, Logo, Uniform, Taskbar, Boundary, Outer, Global on).
Toggling Uniform Brightness wrote `0x7829` and back `0x7869` — verified.

## 6. Settings import/export (`ImportExportManager.cs`)

Codes the app saves per profile, which is a good "everything that matters" list:
- Gaming: DC E2 14 E6 10 12 16 18 1A 59–5F 72 87 8A 90 E0 E1 E3 E5 EA EE FC FD
- ProArt: E3 14 E6 10 12 16 18 1A 6B 6C 6E 70 72 87 8A 90 E0 E1 E4 E5 F2 FC FD

## 7. Answered by a full decompile audit

A later audit of the whole decompiled app resolved most of the earlier unknowns:

- **0xFC** — the app never reads or writes individual bits; `CapabilityConfigManager` reads the caps
  token: low byte bit `0x08` = Audio Mute, `0x10` = Auto Source Detection. Other bits unknown.
- **0xE4 timer** — 1–5 = 30/40/50/60/90 minutes.
- **0xE8** — a write-only position nudge (1 Up, 2 Down, 3 Right, 4 Left), not a value you read back.
- **0xED** — Rest Reminder, 0 off then 1–12 = 5–60 minutes.
- **0xAA** — orientation, read-only: 1/0xF1 = 0°, 2/0xF3 = 90°, 3/0xF2 = 180°, 4/0xF0 = 270°.
- **0x79** — MCU firmware (ProArt self-calibration models); 254 = none.
- **0xE2 HDR** — high byte is the signal group (1 HDR10, 2 Dolby Vision); the app never writes `0x0000`.
- **ProArt presets (0xE3)** — SDR presets are `preset << 8 | 0xFF`; HDR presets are full values
  (0x0F00/0x1000/0x1400, 0x1303/0x1304); HLG (0x11FF/0x12FF) is a two-step write.
- **Colour temperature (0x14)** — the same codes have different names per product line.
- **Crosshair (0xE3)** — 1 Red Dot, 2 Green Dot, 3/4 Red/Green Bullseye, 5/6 Rangefinder,
  7 Blue Dot, 8 Green Dot, 9/10 Mini Duplex, 11/12 Heavy Duplex. 13–15 unnamed.

Still open: 0xF7, 0xFA, 0xDD, 0x5F, 0x61 and the `0x0100/0x0200` high bytes in 0xDC are enum names
or caps tokens the app never decodes. To probe one safely: `asusdisplay raw 0xNN`, change the
setting in the monitor's OSD, and read it again.
