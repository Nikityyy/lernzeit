# Lernzeit

I study with Spotify playing and used the Windows Clock app to time it. Two things bothered me. The Clock app shows one number and no history, so I never knew how much I studied this week. And when I paused the music for a break, I often forgot to pause the clock, so the time was wrong.

Lernzeit is a study timer that follows Spotify. Pause the music and the timer pauses at the same second. Play it again and the timer goes on. It keeps every session and shows how much you studied per day, week, month and year, at what time of day, and which songs, artists and podcasts you listened to while studying.

<p>
  <img src="docs/timer.png" alt="The timer" width="49%">
  <img src="docs/stats.png" alt="The month statistics" width="49%">
</p>

The app is in German.

## Install

Windows 10 or 11. Download `Lernzeit.exe` from the [latest release](https://github.com/Nikityyy/lernzeit/releases/latest) and run it. There is no installer. It keeps its data in `%APPDATA%\Lernzeit`.

To build it yourself, install [Rust](https://rustup.rs) and run `cargo build --release`.

## Use

Press the red button or the space bar to start a session. Lernzeit never starts one on its own; music alone does not count as study time.

- **Spotify pauses**: the timer pauses at the moment the music stopped.
- **Spotify plays again**: the timer goes on, but only if Spotify paused it. A timer you paused yourself stays paused.
- **Space bar**: in Settings you choose what it pauses: the timer and the music, only the timer (the music keeps playing), or only the music.
- **Closing the window** keeps Lernzeit in the tray. Quit it from the tray menu.

A study day ends at 4:00 in the morning, so a session past midnight counts for the day you started it. Spotify Free ads are not counted as music.

On first launch, Lernzeit adds the time from the Windows Clock stopwatch as one session, so the time you already tracked is not lost.

## How it works

Lernzeit reads Spotify through Windows' media controls, the same ones behind the media keys and the volume flyout. It needs no Spotify account or login, and it works with the Spotify desktop app only.

Sessions and listening go into two JSON Lines files, one line per stretch. A running session is saved every 30 seconds, so a crash or shutdown loses at most that much.

## License

MIT
