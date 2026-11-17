//! Everything Lernzeit remembers, as small JSON files in `%APPDATA%\Lernzeit`
//! (or `LERNZEIT_HOME`, for testing).
//!
//! - `study.jsonl`: one line per stretch of study time.
//! - `listening.jsonl`: one line per stretch of music or podcast heard while studying.
//! - `settings.json`: the settings.
//! - `running.json`: the stretch in progress, rewritten every 30 seconds, so a crash or
//!   shutdown loses at most that much.
//! - `covers/`: album art, one file per album.

use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// Milliseconds since the Unix epoch.
pub type Ms = i64;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Study {
    pub start: Ms,
    pub end: Ms,
    /// Taken over from the Windows Clock stopwatch.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub imported: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Song,
    Podcast,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Listen {
    pub start: Ms,
    pub end: Ms,
    pub kind: Kind,
    pub title: String,
    /// The artist of a song, or the show of a podcast episode.
    pub artist: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub album: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SpaceMode {
    #[default]
    Both,
    TimerOnly,
    MusicOnly,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub space: SpaceMode,
    pub clock_imported: bool,
}

#[derive(Serialize, Deserialize)]
struct Running {
    start: Ms,
    seen: Ms,
}

pub struct Store {
    dir: PathBuf,
    pub study: Vec<Study>,
    pub listening: Vec<Listen>,
    pub settings: Settings,
}

impl Store {
    pub fn open() -> Self {
        Self::open_at(
            std::env::var_os("LERNZEIT_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Lernzeit")))
                .unwrap_or_else(|| PathBuf::from("Lernzeit")),
        )
    }

    pub fn open_at(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(dir.join("covers"));
        let mut store = Self {
            study: read_lines(&dir.join("study.jsonl")),
            listening: read_lines(&dir.join("listening.jsonl")),
            settings: fs::read(dir.join("settings.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default(),
            dir,
        };
        // A stretch that was running when Lernzeit stopped ends where it was last seen.
        if let Some(running) = fs::read(store.dir.join("running.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Running>(&b).ok())
        {
            store.add_study(Study { start: running.start, end: running.seen, imported: false });
            store.clear_running();
        }
        store
    }

    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }

    pub fn cover_path(&self, key: &str) -> PathBuf {
        self.dir.join("covers").join(format!("{key}.img"))
    }

    pub fn add_study(&mut self, study: Study) {
        if study.end - study.start >= 1000 {
            append(&self.dir.join("study.jsonl"), &study);
            self.study.push(study);
        }
    }

    pub fn add_listen(&mut self, listen: Listen) {
        if listen.end - listen.start >= 1000 {
            append(&self.dir.join("listening.jsonl"), &listen);
            self.listening.push(listen);
        }
    }

    pub fn save_settings(&self) {
        if let Ok(json) = serde_json::to_vec_pretty(&self.settings) {
            let _ = fs::write(self.dir.join("settings.json"), json);
        }
    }

    pub fn mark_running(&self, start: Ms, seen: Ms) {
        if let Ok(json) = serde_json::to_vec(&Running { start, seen }) {
            let _ = fs::write(self.dir.join("running.json"), json);
        }
    }

    pub fn clear_running(&self) {
        let _ = fs::remove_file(self.dir.join("running.json"));
    }
}

fn read_lines<T: for<'de> Deserialize<'de>>(path: &PathBuf) -> Vec<T> {
    let Ok(file) = fs::File::open(path) else { return Vec::new() };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect()
}

fn append<T: Serialize>(path: &PathBuf, value: &T) {
    let Ok(mut line) = serde_json::to_string(value) else { return };
    line.push('\n');
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

/// A fresh, empty data folder for one test.
#[cfg(test)]
pub fn test_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir()
        .join("lernzeit-tests")
        .join(format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = fs::remove_dir_all(&dir);
    dir
}
