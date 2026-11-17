//! Takes over the time on the Windows Clock (Uhr) stopwatch, once.
//!
//! Clock keeps its settings in a private registry hive. The stopwatch is the value
//! `LocalState\Stopwatch\Stopwatch`, a serialized `ApplicationDataCompositeValue`:
//! entries of `[size u32][type u32][name length u32][UTF-16 name + NUL][value]`, each
//! starting on an 8-byte boundary. `ElapsedTime` is an i64 in 100 ns ticks and
//! `__Updated` a FILETIME of the last change.

use crate::store::Study;
use windows::core::{w, HSTRING};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{RegCloseKey, RegGetValueW, RegLoadAppKeyW, HKEY, KEY_READ, RRF_RT_ANY};

pub enum Import {
    /// The stopwatch held this much time.
    Found(Study),
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
    let mut key = HKEY::default();
    let path = HSTRING::from(hive.as_os_str());
    // REG_PROCESS_APPKEY (1) keeps the hive private to this process.
    if unsafe { RegLoadAppKeyW(&path, &mut key, KEY_READ.0, 1, None) } != ERROR_SUCCESS {
        return Import::Busy;
    }
    let mut data = vec![0u8; 4096];
    let mut len = data.len() as u32;
    let read = unsafe {
        RegGetValueW(key, w!(r"LocalState\Stopwatch"), w!("Stopwatch"), RRF_RT_ANY, None,
            Some(data.as_mut_ptr().cast()), Some(&mut len))
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    if read != ERROR_SUCCESS {
        return Import::Nothing;
    }
    data.truncate(len as usize);
    parse(&data).map_or(Import::Nothing, Import::Found)
}

/// Reads the stopwatch value. `None` when it holds no time.
fn parse(data: &[u8]) -> Option<Study> {
    let (mut elapsed, mut updated) = (None, None);
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
            let v = i64::from_le_bytes(data[value..value + 8].try_into().unwrap());
            match String::from_utf16_lossy(&name).as_str() {
                "ElapsedTime" => elapsed = Some(v),
                "__Updated" => updated = Some(v),
                _ => {}
            }
        }
        at = (at + size + 7) & !7;
    }
    let elapsed = elapsed? / 10_000;
    // FILETIME counts 100 ns from 1601; Unix time starts 11 644 473 600 s later.
    let end = updated? / 10_000 - 11_644_473_600_000;
    (elapsed > 0).then_some(Study { start: end - elapsed, end, imported: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The stopwatch value from a real Clock hive: 31.9 seconds, saved in 2026.
    /// The 4 bytes after ElapsedTime are leftover padding, not data.
    const REAL: &str = "200000000500000007000000560065007200730069006f006e000000010000002c000000060000000b00000045006c0061007000730065006400540069006d0065000000df9e0113000000007b13819b28000000060000000900000053007400610072007400540069006d006500000000000000000000002800000006000000090000005f005f004300720065006100740065006400000088a35b453ac8dc012800000006000000090000005f005f0055007000640061007400650064000000ea22e7f26562dd01";

    #[test]
    fn reads_a_real_stopwatch() {
        let data: Vec<u8> = (0..REAL.len()).step_by(2).map(|i| u8::from_str_radix(&REAL[i..i + 2], 16).unwrap()).collect();
        let study = parse(&data).unwrap();
        assert_eq!(study.end - study.start, 31_887);
        assert_eq!(crate::stats::local(study.end).format("%Y").to_string(), "2026");
        assert!(study.imported);
    }
}
