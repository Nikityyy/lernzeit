//! Follows the Spotify desktop app through Windows' media session (the same source as the
//! volume flyout), and plays or pauses it on request. No Spotify login is needed.
//!
//! Windows reports changes as events; a thread waits for them and also checks every few
//! seconds, so it notices Spotify starting or closing.

use crate::core::{Command, Core, Track};
use crate::store::Ms;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use windows::core::HSTRING;
use windows::Foundation::TypedEventHandler;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;

/// A stop must last this long before the timer pauses; a track change can look like a
/// short stop.
const SETTLE: Ms = 1500;
const RESCAN: Duration = Duration::from_secs(3);

enum Wake {
    Changed,
    Do(Command),
}

#[derive(Clone)]
pub struct Remote(Sender<Wake>);

impl Remote {
    pub fn send(&self, command: Command) {
        let _ = self.0.send(Wake::Do(command));
    }
}

/// Starts the thread. `changed` runs after every change, to redraw the window.
pub fn start(core: Arc<Mutex<Core>>, changed: impl Fn() + Send + 'static) -> Remote {
    let (tx, rx) = channel();
    let remote = Remote(tx.clone());
    std::thread::spawn(move || {
        if let Err(e) = run(core, tx, rx, changed) {
            eprintln!("spotify: {e}");
        }
    });
    remote
}

fn run(core: Arc<Mutex<Core>>, tx: Sender<Wake>, rx: Receiver<Wake>, changed: impl Fn()) -> windows::core::Result<()> {
    let manager = Manager::RequestAsync()?.join()?;
    let wake = tx.clone();
    manager.SessionsChanged(&TypedEventHandler::new(move |_, _| {
        let _ = wake.send(Wake::Changed);
        Ok(())
    }))?;

    let mut session: Option<(Session, HSTRING)> = None;
    let mut playing = false;
    // When Spotify stopped, while we wait to see whether it stays stopped.
    let mut stopped_at: Option<Ms> = None;
    let mut track: Option<Track> = None;

    loop {
        // Find Spotify's session (Store app or classic installer) and listen to it.
        let found = find_spotify(&manager);
        if found.as_ref().map(|s| &s.1) != session.as_ref().map(|s| &s.1) {
            if let Some((s, _)) = &found {
                let (a, b) = (tx.clone(), tx.clone());
                s.PlaybackInfoChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = a.send(Wake::Changed);
                    Ok(())
                }))?;
                s.MediaPropertiesChanged(&TypedEventHandler::new(move |_, _| {
                    let _ = b.send(Wake::Changed);
                    Ok(())
                }))?;
            }
            session = found;
        }

        let now = now();
        let status = session.as_ref().and_then(|(s, _)| s.GetPlaybackInfo().ok()?.PlaybackStatus().ok());
        let is_playing = status == Some(Status::Playing);
        let new_track = session.as_ref().and_then(|(s, _)| read_track(s));
        {
            let mut core = core.lock().unwrap();
            core.spotify_found = session.is_some();
            if new_track != track {
                if let (Some(t), Some((s, _))) = (&new_track, &session) {
                    save_cover(s, &core.store.cover_path(&t.cover_key()));
                }
                track = new_track.clone();
                core.spotify_track(new_track, now);
            }
            if is_playing {
                stopped_at = None;
                if !playing {
                    playing = true;
                    core.spotify_playing(true, now);
                }
            } else if playing {
                let since = *stopped_at.get_or_insert(now);
                if now - since >= SETTLE {
                    playing = false;
                    stopped_at = None;
                    core.spotify_playing(false, since);
                }
            }
        }
        changed();

        let wait = if stopped_at.is_some() { Duration::from_millis(SETTLE as u64 / 3) } else { RESCAN };
        match rx.recv_timeout(wait) {
            Ok(Wake::Do(command)) => perform(&session, command),
            Ok(Wake::Changed) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        // Several events often arrive together; handle them in one pass.
        while let Ok(extra) = rx.try_recv() {
            if let Wake::Do(command) = extra {
                perform(&session, command);
            }
        }
    }
}

fn perform(session: &Option<(Session, HSTRING)>, command: Command) {
    if let Some((s, _)) = session {
        let _ = match command {
            Command::Play => s.TryPlayAsync().and_then(|op| op.join()),
            Command::Pause => s.TryPauseAsync().and_then(|op| op.join()),
        };
    }
}

fn find_spotify(manager: &Manager) -> Option<(Session, HSTRING)> {
    let sessions = manager.GetSessions().ok()?;
    (0..sessions.Size().ok()?).find_map(|i| {
        let s = sessions.GetAt(i).ok()?;
        let id = s.SourceAppUserModelId().ok()?;
        id.to_string().to_lowercase().contains("spotify").then_some((s, id))
    })
}

fn read_track(s: &Session) -> Option<Track> {
    let p = s.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let track = Track {
        title: p.Title().ok()?.to_string(),
        artist: p.Artist().map(|a| a.to_string()).unwrap_or_default(),
        album: p.AlbumTitle().map(|a| a.to_string()).unwrap_or_default(),
    };
    (!track.title.is_empty()).then_some(track)
}

/// Saves the cover art once per album.
fn save_cover(s: &Session, path: &std::path::Path) {
    if path.exists() {
        return;
    }
    let bytes = (|| -> windows::core::Result<Vec<u8>> {
        let props = s.TryGetMediaPropertiesAsync()?.join()?;
        let stream = props.Thumbnail()?.OpenReadAsync()?.join()?;
        let size = stream.Size()? as u32;
        let reader = DataReader::CreateDataReader(&stream)?;
        reader.LoadAsync(size)?.join()?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes)?;
        Ok(bytes)
    })();
    if let Ok(bytes) = bytes {
        if !bytes.is_empty() {
            let _ = std::fs::write(path, bytes);
        }
    }
}

pub fn now() -> Ms {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as Ms)
}
