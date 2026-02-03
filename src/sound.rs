//! Sound handling for the Minesweeper game, including tune playback and sound state management.

use core::ptr::{null, null_mut};

use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_PURGE, SND_RESOURCE};
use winsafe::HINSTANCE;

/// Logical UI tunes that map to embedded wave resources.
#[repr(u16)]
pub enum Sound {
    /// Short tick used for timer and click feedback.
    Tick = 432,
    /// Win jingle played after successfully clearing the board.
    WinGame = 433,
    /// Loss sound played after detonating a mine.
    LoseGame = 434,
}

impl Sound {
    /// Play a specific UI tune using the sounds in the resource file
    /// # Arguments
    /// * `tune` - The tune to play
    pub fn play(self, hinst: &HINSTANCE) {
        let resource_ptr = self as usize as *const u16;
        // Playback uses the async flag so the UI thread is never blocked.
        unsafe {
            PlaySoundW(resource_ptr, hinst.ptr(), SND_RESOURCE | SND_ASYNC);
        }
    }

    /// Initialize the sound system and determine whether sound effects can be played.
    /// # Returns
    /// `true` if sound effects can be played, `false` otherwise.
    pub fn init() -> bool {
        // Attempt to stop any playing sounds; if the API fails we assume the
        // machine cannot play audio and disable sound effects in preferences.
        Self::stop_all()
    }

    /// Stop all currently playing sounds.
    /// # Returns
    /// `true` if the operation succeeded, `false` otherwise.
    pub fn stop_all() -> bool {
        // Passing NULL tells PlaySound to purge the current queue.
        unsafe { PlaySoundW(null(), null_mut(), SND_PURGE) != 0 }
    }
}
