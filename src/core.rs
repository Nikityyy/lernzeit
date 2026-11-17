//! The timer, and how it follows Spotify.
//!
//! Rules:
//! - You start a session yourself. Music alone never counts as study time.
//! - When Spotify stops while the timer runs, the timer pauses at the moment the music
//!   stopped. When Spotify plays again, the timer resumes, but only if Spotify paused it.
//! - Music heard while the timer runs goes into the listening stats.

use crate::store::{Kind, Listen, Ms, SpaceMode, Store, Study};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    Stopped,
    Running { since: Ms },
    Paused { by: PausedBy },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PausedBy {
    You,
    Spotify,
}

/// What Spotify is playing, as Windows reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
}

impl Track {
    /// Songs have an artist. Podcast episodes have none and carry the show as the album.
    /// Ads (Spotify Free) have neither.
    pub fn kind(&self) -> Option<Kind> {
        match (self.artist.is_empty(), self.album.is_empty()) {
            (false, _) => Some(Kind::Song),
            (true, false) => Some(Kind::Podcast),
            (true, true) => None,
        }
    }

    /// The artist of a song, or the show of a podcast.
    pub fn author(&self) -> &str {
        if self.artist.is_empty() { &self.album } else { &self.artist }
    }

    pub fn cover_key(&self) -> String {
        cover_key(self.author(), &self.album)
    }
}

/// The file name of an album's cover art: an FNV-1a hash of the artist (or show) and album.
pub fn cover_key(author: &str, album: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in author.bytes().chain([0]).chain(album.bytes()) {
        hash = (hash ^ byte as u64).wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// A request for the Spotify thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Play,
    Pause,
}

pub struct Core {
    pub store: Store,
    pub run: Run,
    /// Study time of the current session before the stretch that is running now.
    session_before: Ms,
    pub track: Option<Track>,
    pub playing: bool,
    pub spotify_found: bool,
    /// A play state we asked Spotify for without the timer following ("music only").
    /// Spotify's report of it is not a reason to pause or resume the timer.
    expect: Option<(bool, Ms)>,
    listening: Option<(Track, Ms)>,
    last_mark: Ms,
}

impl Core {
    pub fn new(store: Store) -> Self {
        Self {
            store,
            run: Run::Stopped,
            session_before: 0,
            track: None,
            playing: false,
            spotify_found: false,
            expect: None,
            listening: None,
            last_mark: 0,
        }
    }

    /// Study time of the current session.
    pub fn session(&self, now: Ms) -> Ms {
        self.session_before + self.running_since().map_or(0, |since| now - since)
    }

    pub fn running_since(&self) -> Option<Ms> {
        match self.run {
            Run::Running { since } => Some(since),
            _ => None,
        }
    }

    /// The main button and the space bar.
    pub fn toggle(&mut self, now: Ms) -> Option<Command> {
        let mode = self.store.settings.space;
        if mode == SpaceMode::MusicOnly && self.run != Run::Stopped {
            return self.toggle_music(now);
        }
        let music = mode != SpaceMode::TimerOnly && self.spotify_found;
        match self.run {
            Run::Running { .. } => {
                self.pause(now, PausedBy::You);
                (music && self.playing).then_some(Command::Pause)
            }
            Run::Stopped | Run::Paused { .. } => {
                if self.run == Run::Stopped {
                    self.session_before = 0;
                }
                self.resume(now);
                (music && !self.playing).then_some(Command::Play)
            }
        }
    }

    /// Plays or pauses Spotify without touching the timer.
    pub fn toggle_music(&mut self, now: Ms) -> Option<Command> {
        if !self.spotify_found {
            return None;
        }
        self.expect = Some((!self.playing, now + 3000));
        Some(if self.playing { Command::Pause } else { Command::Play })
    }

    /// Ends the session.
    pub fn stop(&mut self, now: Ms) {
        if let Run::Running { since } = self.run {
            self.close_study(since, now);
        }
        self.run = Run::Stopped;
        self.session_before = 0;
        self.sync_listening(now);
    }

    /// Spotify started or stopped playing at `at`. A stop is reported only once it has
    /// lasted a moment, so `at` may lie a little in the past.
    pub fn spotify_playing(&mut self, playing: bool, at: Ms) {
        self.playing = playing;
        let expected = self.expect.take().is_some_and(|(state, until)| state == playing && at <= until);
        if !expected {
            match (playing, self.run) {
                (false, Run::Running { .. }) => self.pause(at, PausedBy::Spotify),
                (true, Run::Paused { by: PausedBy::Spotify }) => self.resume(at),
                _ => {}
            }
        }
        self.sync_listening(at);
    }

    pub fn spotify_track(&mut self, track: Option<Track>, at: Ms) {
        self.track = track;
        self.sync_listening(at);
    }

    /// Saves the running stretch every 30 seconds, so a crash loses at most that.
    pub fn tick(&mut self, now: Ms) {
        if let Some(since) = self.running_since() {
            if now - self.last_mark >= 30_000 {
                self.last_mark = now;
                self.store.mark_running(since, now);
            }
        }
    }

    /// Closes the open stretches, for quitting.
    pub fn shutdown(&mut self, now: Ms) {
        if let Run::Running { since } = self.run {
            self.close_study(since, now);
            self.run = Run::Paused { by: PausedBy::You };
        }
        self.sync_listening(now);
    }

    fn pause(&mut self, at: Ms, by: PausedBy) {
        if let Run::Running { since } = self.run {
            let at = at.max(since);
            self.close_study(since, at);
            self.session_before += at - since;
        }
        self.run = Run::Paused { by };
        self.sync_listening(at);
    }

    fn resume(&mut self, at: Ms) {
        self.run = Run::Running { since: at };
        self.last_mark = at;
        self.store.mark_running(at, at);
        self.sync_listening(at);
    }

    fn close_study(&mut self, start: Ms, end: Ms) {
        self.store.add_study(Study { start, end, imported: false });
        self.store.clear_running();
    }

    /// Opens or closes the listening stretch so it covers exactly the time the timer
    /// runs while Spotify plays a song or podcast.
    fn sync_listening(&mut self, at: Ms) {
        let want = match (&self.run, self.playing, &self.track) {
            (Run::Running { .. }, true, Some(track)) if track.kind().is_some() => Some(track.clone()),
            _ => None,
        };
        if self.listening.as_ref().map(|(t, _)| t) == want.as_ref() {
            return;
        }
        if let Some((track, start)) = self.listening.take() {
            if let Some(kind) = track.kind() {
                self.store.add_listen(Listen {
                    start,
                    end: at.max(start),
                    kind,
                    artist: track.author().to_owned(),
                    album: track.album,
                    title: track.title,
                });
            }
        }
        self.listening = want.map(|track| (track, at));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core() -> Core {
        let dir = crate::store::test_dir();
        let mut core = Core::new(Store::open_at(dir));
        core.spotify_found = true;
        core
    }

    fn song(title: &str) -> Option<Track> {
        Some(Track { title: title.into(), artist: "Artist".into(), album: "Album".into() })
    }

    #[test]
    fn classifies_songs_podcasts_and_ads() {
        let t = |a: &str, b: &str| Track { title: "x".into(), artist: a.into(), album: b.into() }.kind();
        assert_eq!(t("Johann Strauss I", "Soundtrack"), Some(Kind::Song));
        assert_eq!(t("", "Minecraft music and Ambience"), Some(Kind::Podcast));
        assert_eq!(t("", ""), None);
    }

    #[test]
    fn spotify_pause_pauses_and_resumes_the_timer() {
        let mut c = core();
        c.spotify_playing(true, 0);
        assert_eq!(c.toggle(0), None, "music already plays");
        c.spotify_playing(false, 60_000);
        assert_eq!(c.run, Run::Paused { by: PausedBy::Spotify });
        c.spotify_playing(true, 90_000);
        assert_eq!(c.run, Run::Running { since: 90_000 });
        assert_eq!(c.session(100_000), 70_000);
        assert_eq!(c.store.study, vec![Study { start: 0, end: 60_000, imported: false }]);
    }

    #[test]
    fn spotify_does_not_resume_a_timer_you_paused() {
        let mut c = core();
        c.spotify_playing(true, 0);
        c.store.settings.space = SpaceMode::TimerOnly;
        c.toggle(0);
        c.toggle(10_000);
        c.spotify_playing(false, 20_000);
        c.spotify_playing(true, 30_000);
        assert_eq!(c.run, Run::Paused { by: PausedBy::You });
    }

    #[test]
    fn music_starting_alone_never_starts_the_timer() {
        let mut c = core();
        c.spotify_playing(true, 0);
        assert_eq!(c.run, Run::Stopped);
    }

    #[test]
    fn space_in_both_mode_controls_spotify() {
        let mut c = core();
        assert_eq!(c.toggle(0), Some(Command::Play));
        c.spotify_playing(true, 500);
        assert_eq!(c.toggle(5_000), Some(Command::Pause));
        c.spotify_playing(false, 5_200);
        assert_eq!(c.run, Run::Paused { by: PausedBy::You });
    }

    #[test]
    fn music_only_keeps_the_timer_running() {
        let mut c = core();
        c.spotify_playing(true, 0);
        c.toggle(0);
        assert_eq!(c.toggle_music(1_000), Some(Command::Pause));
        c.spotify_playing(false, 1_300);
        assert_eq!(c.run, Run::Running { since: 0 });
        // A later stop that we did not ask for pauses the timer again.
        assert_eq!(c.toggle_music(2_000), Some(Command::Play));
        c.spotify_playing(true, 2_100);
        c.spotify_playing(false, 9_000);
        assert_eq!(c.run, Run::Paused { by: PausedBy::Spotify });
    }

    #[test]
    fn listening_counts_only_while_the_timer_runs() {
        let mut c = core();
        c.spotify_track(song("A"), 0);
        c.spotify_playing(true, 0);
        c.toggle(10_000);
        c.spotify_track(song("B"), 40_000);
        c.spotify_playing(false, 70_000);
        let heard: Vec<_> = c.store.listening.iter().map(|l| (l.title.as_str(), l.start, l.end)).collect();
        assert_eq!(heard, vec![("A", 10_000, 40_000), ("B", 40_000, 70_000)]);
    }

    #[test]
    fn ads_are_not_counted_as_music() {
        let mut c = core();
        c.spotify_playing(true, 0);
        c.toggle(0);
        c.spotify_track(Some(Track { title: "Spotify".into(), artist: String::new(), album: String::new() }), 0);
        c.spotify_track(song("A"), 30_000);
        c.stop(60_000);
        assert_eq!(c.store.listening.len(), 1);
        assert_eq!(c.store.study, vec![Study { start: 0, end: 60_000, imported: false }]);
    }
}
