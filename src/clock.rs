//! Takes over the focus sessions from the Windows Clock (Uhr) app, once.
//!
//! Clock keeps its settings in a private registry hive. Each finished focus stretch is a
//! value under `LocalState\FocusSessionRecord\JustFocusContainer`, a serialized
//! `ApplicationDataCompositeValue`: entries of
//! `[size u32][type u32][name length u32][UTF-16 name + NUL][value]`, each starting on an
//! 8-byte boundary. `TotalSeconds` is an f64 and `ModifiedDateTime` the end of the
//! stretch in Unix seconds (i64).

use crate::store::{Ms, Study};
use windows::core::{w, HSTRING, PWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{RegCloseKey, RegEnumValueW, RegLoadAppKeyW, RegOpenKeyExW, HKEY, KEY_READ};

pub enum Import {
    /// The focus stretches Clock recorded, oldest first.
    Found(Vec<Study>),
    /// There is nothing to take over.
    Nothing,
    /// Clock is open and holds the file; try again next launch.
    Busy,
}

pub fn import() -> Import {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else { return Import::Nothing };
    let hive = std::path::Path::new(&local)
        .join(r"Packages\Microsoft.WindowsAlarms_8wekyb3d8bbwe\Settings\settings.dat");
    if !hive.exists() {
        return Import::Nothing;
    }
    let mut root = HKEY::default();
    // REG_PROCESS_APPKEY (1) keeps the hive private to this process.
    if unsafe { RegLoadAppKeyW(&HSTRING::from(hive.as_os_str()), &mut root, KEY_READ.0, 1, None) } != ERROR_SUCCESS {
        return Import::Busy;
    }
    let mut records = Vec::new();
    let mut key = HKEY::default();
    unsafe {
        if RegOpenKeyExW(root, w!(r"LocalState\FocusSessionRecord\JustFocusContainer"), None, KEY_READ, &mut key) == ERROR_SUCCESS {
            let mut name = [0u16; 256];
            let mut data = vec![0u8; 4096];
            for index in 0.. {
                let (mut name_len, mut data_len) = (name.len() as u32, data.len() as u32);
                let read = RegEnumValueW(key, index, Some(PWSTR(name.as_mut_ptr())), &mut name_len, None, None,
                    Some(data.as_mut_ptr()), Some(&mut data_len));
                if read != ERROR_SUCCESS {
                    break;
                }
                records.extend(parse(&data[..data_len as usize]));
            }
            let _ = RegCloseKey(key);
        }
        let _ = RegCloseKey(root);
    }
    let studies = stretches(records);
    if studies.is_empty() { Import::Nothing } else { Import::Found(studies) }
}

/// One focus record: its length and end, in ms. `None` when it holds no time.
fn parse(data: &[u8]) -> Option<(Ms, Ms)> {
    let (mut seconds, mut end) = (None, None);
    let mut at = 0;
    while at + 12 <= data.len() {
        let u32_at = |i: usize| u32::from_le_bytes(data[i..i + 4].try_into().unwrap()) as usize;
        let (size, name_len) = (u32_at(at), u32_at(at + 8));
        let name_end = at + 12 + name_len * 2;
        if size < 12 || name_end + 2 > data.len() || at + size > data.len() {
            break;
        }
        let name: Vec<u16> = data[at + 12..name_end].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let value = name_end + 2;
        if value + 8 <= at + size {
            let bytes: [u8; 8] = data[value..value + 8].try_into().unwrap();
            match String::from_utf16_lossy(&name).as_str() {
                "TotalSeconds" => seconds = Some(f64::from_le_bytes(bytes)),
                "ModifiedDateTime" => end = Some(i64::from_le_bytes(bytes)),
                _ => {}
            }
        }
        at = (at + size + 7) & !7;
    }
    let (length, end) = ((seconds? * 1000.0).round() as Ms, end? * 1000);
    (length > 0).then_some((length, end))
}

/// Turns records into study stretches, oldest first. The end times are whole seconds,
/// so a stretch may reach a moment into the one before it; it starts where that one ended.
fn stretches(mut records: Vec<(Ms, Ms)>) -> Vec<Study> {
    records.sort_by_key(|&(_, end)| end);
    let mut previous_end = Ms::MIN;
    records
        .into_iter()
        .filter_map(|(length, end)| {
            let start = (end - length).max(previous_end);
            previous_end = end;
            (end > start).then_some(Study { start, end, imported: true })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect()
    }

    /// A focus record from a real Clock hive: 52.2 seconds, ended 17 Nov 2026 at 12:17:54 CET.
    const REAL: &str = "6c0000000c0000000e00000046006f00630075007300530065007300730069006f006e00490064000000330037006400650034003700320066006400350030006200340065006500310039006100360039006600310062003400640037006400340063003600310032000000310030002e000000090000000c00000054006f00740061006c005300650063006f006e006400730000008e1f2a8d981c4a400000300000000c000000100000004100730073006f006300690061007400650064005400610073006b0049006400000000003600000006000000100000004d006f006400690066006900650064004400610074006500540069006d0065000000e237fc6a000000000000103db54a8576dd01";

    #[test]
    fn reads_a_real_focus_record() {
        let (length, end) = parse(&bytes(REAL)).unwrap();
        assert_eq!(length, 52_223);
        assert_eq!(end, 1_794_914_274_000);
        assert_eq!(crate::stats::local(end).format("%Y-%m-%d").to_string(), "2026-11-17");
    }

    #[test]
    fn stretches_never_overlap() {
        let studies = stretches(vec![(95_840, 1_000_000), (52_224, 903_000)]);
        let spans: Vec<_> = studies.iter().map(|s| (s.start, s.end)).collect();
        assert_eq!(spans, vec![(850_776, 903_000), (904_160, 1_000_000)]);
        let studies = stretches(vec![(60_000, 1_000_000), (10_000, 950_000)]);
        assert_eq!(studies[1].start, 950_000, "starts where the one before ended");
    }
}
