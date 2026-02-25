//! Sound handling for the Minesweeper game, including tune playback and sound state management.

use core::ptr::{null, null_mut};

use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_PURGE, SND_RESOURCE};
use winsafe::{HINSTANCE, IdStr};

use crate::util::ResourceId;

/// Logical UI tunes that map to embedded wave resources.
///
/// TODO: Should the Sound enum exist?
pub enum Sound {
    /// Short tick used for timer and click feedback.
    Tick = ResourceId::TuneTick as isize,
    /// Win jingle played after successfully clearing the board.
    WinGame = ResourceId::TuneWon as isize,
    /// Loss sound played after detonating a mine.
    LoseGame = ResourceId::TuneLost as isize,
}

impl Sound {
    /// Play a specific UI tune using the sounds in the resource file
    /// # Arguments
    /// - `hinst` - The HINSTANCE of the current process, used to locate the sound resource.
    pub fn play(self, hinst: &HINSTANCE) {
        let resource_ptr = IdStr::Id(self as u16).as_ptr();
        // Playback uses the async flag so the UI thread is never blocked
        // Failures are ignored since sound is a non-essential feature
        unsafe {
            PlaySoundW(resource_ptr, hinst.ptr(), SND_RESOURCE | SND_ASYNC);
        }
    }

    /// Reset the sound system by stopping any currently playing sounds.
    /// # Returns
    /// - `true` - If the sound API successfully stopped all sounds, indicating that sounds can be played without issue.
    /// - `false` - If the sound API failed to stop sounds, indicating a potential issue with the sound system.
    pub fn reset() -> bool {
        // Passing NULL tells PlaySound to purge the current queue.
        unsafe { PlaySoundW(null(), null_mut(), SND_PURGE) != 0 }
    }
}
