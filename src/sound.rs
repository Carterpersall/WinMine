/// Quick helpers for the small set of winmm-backed tunes used by the UI.
use core::ptr::{null, null_mut};

use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_PURGE, SND_RESOURCE};
use winsafe::HINSTANCE;

use crate::pref::SoundState;

/// Logical UI tunes that map to embedded wave resources.
#[repr(u16)]
pub enum Tune {
    /// Short tick used for timer and click feedback.
    Tick = 432,
    /// Win jingle played after successfully clearing the board.
    WinGame = 433,
    /// Loss sound played after detonating a mine.
    LoseGame = 434,
}

/// Initialize the sound system and determine whether sound effects are enabled.
/// # Returns
/// A `SoundState` enum indicating whether sound effects can be played.
pub fn FInitTunes() -> SoundState {
    // Attempt to stop any playing sounds; if the API fails we assume the
    // machine cannot play audio and disable sound effects in preferences.
    if stop_all_sounds() {
        SoundState::On
    } else {
        SoundState::Off
    }
}

/// Stop all currently playing sounds.
/// # Returns
/// `true` if the operation succeeded, `false` otherwise.
pub fn stop_all_sounds() -> bool {
    // Passing NULL tells PlaySound to purge the current queue.
    unsafe { PlaySoundW(null(), null_mut(), SND_PURGE) != 0 }
}

/// Play a specific UI tune using the sounds in the resource file
/// # Arguments
/// * `tune` - The tune to play
pub fn PlayTune(hinst: &HINSTANCE, tune: Tune) {
    let resource_ptr = tune as usize as *const u16;
    // Playback uses the async flag so the UI thread is never blocked.
    unsafe {
        PlaySoundW(resource_ptr, hinst.ptr(), SND_RESOURCE | SND_ASYNC);
    }
}
