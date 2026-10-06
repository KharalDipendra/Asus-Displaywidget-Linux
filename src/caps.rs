use std::collections::BTreeMap;

use crate::ProductLine;

/// The `vcp(...)` section of an MCCS capabilities string, e.g. `vcp(10 12 14(05 06) FD(7879))`.
/// ASUS also uses 4-digit tokens: 16-bit values and bit masks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub raw: String,
    /// VCP code -> declared values (empty for continuous controls).
    pub vcp: BTreeMap<u8, Vec<u16>>,
}

impl Capabilities {
    pub fn parse(raw: &str) -> Capabilities {
        let spaced = vcp_section(raw).replace('(', " ( ").replace(')', " ) ");
        let mut tokens = spaced.split_whitespace().peekable();
        let mut vcp = BTreeMap::new();

        while let Some(token) = tokens.next() {
            let Ok(code) = u8::from_str_radix(token, 16) else { continue };
            let mut values = Vec::new();
            if tokens.next_if_eq(&"(").is_some() {
                for t in tokens.by_ref().take_while(|t| *t != ")") {
                    values.extend(u16::from_str_radix(t, 16));
                }
            }
            vcp.insert(code, values);
        }

        Capabilities { raw: raw.to_string(), vcp }
    }
}

fn vcp_section(raw: &str) -> &str {
    let Some(start) = raw.to_ascii_lowercase().find("vcp(").map(|i| i + 4) else { return "" };
    let mut depth = 1;
    for (i, c) in raw[start..].char_indices() {
        depth += match c {
            '(' => 1,
            ')' => -1,
            _ => 0,
        };
        if depth == 0 {
            return &raw[start..start + i];
        }
    }
    &raw[start..]
}

/// Capability strings DisplayWidget Center uses when a monitor doesn't answer the request
/// (DisplayManager.GetCapability).
pub fn fallback(model: &str) -> Option<&'static str> {
    let model = model.trim().to_ascii_lowercase();
    if let Some((_, caps)) = FALLBACKS.iter().find(|(name, _)| *name == model) {
        return Some(caps);
    }
    (ProductLine::from_model(&model) == ProductLine::ProArt).then_some(if model.contains('v') { PROART } else { PROART_WITHOUT_V })
}

const FALLBACKS: [(&str, &str); 4] = [
    ("pg49wcd", "(prot(monitor)type(LCD)model(PG49WCD)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 10 12 14(03 04 05 06 07 08 09 0B) 16 18 1A 52 60(0F 11 1A) 62 72(50 78 96) 86(02 0B) 8A 8D(01 02) AC AE B2 B6 C6 C8 CC(01 02 03 04 05 06 07 08 09 0A 0C 0D 11 12 14 1A 1E 1F 23 30 31) D6(01 05) DC(03 0B 0D 0E 11 12 13 14 20) DF E2(00 01 02 03 04 05) e3(07 08 09 0a 0b 0c) E4(00 01) E6(00 01) E8(01 02 03 04 05 06 07 08) E9(00 01) EA(00 01) EB(00 01) EC(00 06 07 08 09 0A 0B) EE(00 1E 28 32 3C 5A) EF(00 01 02 03 FF) F0(00 01 02 03 04) F2(01 02) F6(00 01 02) FC(87FF) FD(00FD) FF)mswhql(1)asset_eep(40)mccs_ver(2.5))"),
    ("pg34wcdm", "(prot(monitor)type(LCD)model(PG34WCDM)cmds(01 02 03 07 0C E3 F3)vcp(02 03 04 08 10 12 14(05 06 08 0B) 16 18 1A 52 60(0F 11 12 1A) 62 72(50 60 78 90 A0) 86(02 0B) 87 8D(01 02) CC(01 02 03 04 05 06 07 08 09 0A 0C 0D 11 12 14 1A 1E 1F 23 24 26 27) D6(01 05) DC(01 02 04 05 06 07 08 09 0A) DF E0(00 01 02 03 04 05) E1(00 01) E2(01 02 03) E3(00 01 02 03 04 05 06 07 08 09 0A 0B 0C) E4(00 01 02 03 04 05) E5(00 01 02 03) E6(00 01 02 03 04) E7(00 01) E8(01 02 03 04 05 06 07 08) E9 EA(00 01 02) EB(00 01 02 03 04 05 06 07) EC(00 01) EF F0(00 01) F2 F3(02 03) F4 F5 F6 F7 F8(00 02 04 08) F9(00 01 02 03) FA FB FC(849F) FD(00FD) FF) mswhql(1)mccs_ver(2.0)asset_eep(32)mpu_ver(01))"),
    ("pg32ucdm", "(prot(monitor)type(LCD)model(PG32UCDM)cmds(01 02 03 07 0C E3 F3)vcp(04 08 0B 0C 10 12 14(03 04 05 06 07 08 09 0B) 16 18 1A 60(11 12 0F 1A) 72 87 8A CC(01 02 03 04 05 06 07 08 09 0A 0C 0D 11 12 14 1A 1E 1F 23 24 25 26 27) D6(01 05) DC(01 02 04 05 06 07 08 09 0A) E1(00 01) E2(01 02 03) E3(07 08 09 0a 0b 0c) E4(00 01 02 03 04 05) E5(00 01 02 03 04) E6 E7(00 01) E8(01 02 03 04 05 06 07 08) E9 EA(00 01 02) EB(00 01 02 03 04 05 06 07) EC(00 01) EE EF F2 F3(01 02 03) F4(00 01 02 03 04 05) F5 F6 F7 F8(00 02 04 08) F9(00 01 02 03) FA FB FC(87FF) FD(00FD) FF)mswhql(1)asset_eep(40)mccs_ver(2.5))"),
    ("xg27acdng", "(prot(monitor)type(LCD)model(XG27ACDNG)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 10 12 14(03 04 05 06 07 08 09 0B) 16 18 1A 52 60(0F 11 12 1A) 62 72(50 64 78 8C A0) 86(02 0B) 8A 8D(01 02) AC AE B2 B6 C6 C8 CC(01 02 03 04 05 06 07 08 09 0A 0C 0D 11 12 14 1A 1E 1F 23 24 25 26 27) D6(01 05) DC(01 02 04 05 06 07 08 09 0A) DF E1(00 01) E2(00 01 02 03) E3(00 07 08 09 0a 0b 0c) E4(00 01 02 03 04 05) E5(00 01 02 03 04) E6(00 01 02 03 04) E7(00 01) E9 EA(00 01 02) EB(00 01 02 03 04 05 06 07) EC(01 FF) EE(00 01) EF(02 0A) F3(02 03 04) F4(00 01 02 03 04) F5 F6 F7 F8(00 02 04 08) F9 FA FB FC(019F) FD(7879) FF)mswhql(1)asset_eep(40)mccs_ver(2.5))"),
];

const PROART: &str = "(prot(monitor) type(LCD)model(ProArtDisplay) cmds(01 02 03 07 0C F3) vcp(02 04 05 08 0B 0C 0F 10 12 14(04 05 08 0E 0F 11) 16 18 1A 52 60(0F 11 15) 61(01 02 03) 62 6B(00 01) 6C 6E 70 72 82(00 01) 87 8A 90 AA(01 02 03 04) C0 C6 CC(01 02 03 04 05 06 07 08 09 0A 0C 0D 11 12 14 1A 1E 1F 23 24 25 26 27) D6(01 02 04 05) DD(00 01) E0 E1(00 01) E3(01 05 08 09 0A 0B 0E 14 16 17 1F 20) E4(00 01 02 03) E5(0503) E6 E7(00 02 06 60 61 62 63 70 71 72) E9 EA(00 01 02) EB(00 01 02 03 04 05 06 07 08 09 0A 0B) EC(01 FF) EF F2 F4(00 01 02 03 04 05 09 0A FE) F5 F6 F7 F8 FB(01 02 03 04 08 09 0A 0B 0C 0E) FC(183D) FD(1A04) FF)mccs_ver(2.2)asset_eep(32)mpu(01)mswhql(1))";

const PROART_WITHOUT_V: &str = "(prot(monitor) type(LCD)model(ProArtDisplay) cmds(01 02 03 07 0C F3) vcp(02 04 05 08 0B 0C 10 12 14(04 05 08 0E 0F) 16 18 1A 52 60(11 12 0F) 62 6C 6E 70 72 86(01 02 0B) 87(00 0A 14 1E 28 32 3C 46 50 5A 64) 8A 8D(01 02) 90 AC AE B6 C6 C8 CC(01 02 03 04 05 06 07 08 09 0A 0C 0D 11 12 14 1A 1E 1F 23 24 25 26 27) D6(01 05) DF DC(26 0D 27 28 25 24 21 20 0E 0F) E1(00 01) E3(01 05 08 09 0A 0B 0E 0F 10 11 12 16 17 1E) E4(00 01 02 03 04) E6 E7(00 01 02 60 61 62 63 70 71 72) E9 EA(00 01 02) EB(00 01 02 03 04 05 06 07) EC(00 FF) ED(00 01 02 03) EF F2 F3(01 02 03) F4(00 01 02 03 04 05) F5 F7 F8 F9 FB(01 02 03 04 06 07 08 09 0A 0B 0C 0E 0F) FC(081F) FD(1A05) FF)mccs_ver(2.2)asset_eep(32)mpu(01)mswhql(1))";

#[cfg(test)]
pub const XG27ACDMS: &str = "(prot(monitor)type(LCD)model(XG27ACDMS)cmds(01 02 03 07 0C E3 F3)vcp(04 08 0B 0C 10 12 14(03 04 05 06 07 08 09 0B) 16 18 1A 5F(0005) 60(11 0F 1A) 61(01 02) 72 87 8A C0 CC(01 02 03 04 05 06 07 08 09 0A 0C 0D 11 12 14 1A 1E 1F 23 24 25 26 27) D6(01 05) DC(0000 0100 0200 01 02 04 05 06 07 08 09 0A) DD(00 01) E1(00 01) E2(0000 0100 00 01 02 03 04) E3(00 07 08 09 0a 0b 0c 0d 0e 0f) E4(00 01 02 03 04 05) E5(00 01 02 03 04) E6 E7(00 01) E8(01 02 03 04 05 06 07 08) E9 EA(00 01 02) EB(00 01 02 03 04 05 06 07) EC(00 01) EE EF F2 F3(FE) F4(00 01 02 03 04 05) F5 F6 F7 F8(00 02 04 08) F9 FA FB FC(0C1F) FD(7879) FF)mswhql(1)asset_eep(40)mccs_ver(2.5))";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_capability_strings() {
        let caps = Capabilities::parse(XG27ACDMS);
        assert_eq!(caps.vcp[&0xFD], [0x7879]);
        assert_eq!(caps.vcp[&0xE2], [0x0000, 0x0100, 0x00, 0x01, 0x02, 0x03, 0x04]);
        assert_eq!(caps.vcp[&0x60], [0x11, 0x0F, 0x1A]);
        assert!(caps.vcp[&0xE6].is_empty() && caps.vcp.contains_key(&0xFF) && !caps.vcp.contains_key(&0xE0));
        assert!(!caps.vcp.contains_key(&0x01), "nothing leaks in from prot/cmds");

        // VG27AQL1A: spaces inside lists, trailing space before ')'.
        let legacy = Capabilities::parse("(model(VG27A1A)vcp(10 60( 11 12 0F) E1(00 01) EF ) mccs_ver(2.2))");
        assert_eq!(legacy.vcp[&0x60], [0x11, 0x12, 0x0F]);
        assert_eq!(legacy.vcp.keys().copied().collect::<Vec<_>>(), [0x10, 0x60, 0xE1, 0xEF]);
    }

    #[test]
    fn fallbacks() {
        assert_eq!(Capabilities::parse(fallback("PG32UCDM").unwrap()).vcp[&0xFD], [0x00FD]);
        assert_eq!(fallback("PA32UCDM"), Some(PROART_WITHOUT_V));
        assert_eq!(fallback("PA279CV"), Some(PROART));
        assert_eq!(fallback("XG27ACDMS"), None);
    }
}
