//! Sound handling for the Minesweeper game, including tune playback and sound state management.
//! Note: Sound toggling behavior is different from the original game, which only allowed sound
//!       to be toggled when sound was enabled.

use winsafe::{HINSTANCE, IdStr, PlaySound, Snd};

use crate::util::ResourceId;

/// Logical UI tunes that map to embedded wave resources.
pub(crate) enum Sound {
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
    pub(crate) fn play(self, hinst: &HINSTANCE) {
        // Failures are ignored since sound is a non-essential feature
        let _ = PlaySound(Snd::ResAsync {
            id: IdStr::Id(self as u16),
            hinst,
            default: false,
            stop: false,
            sentry: false,
            loops: false,
        });
    }

    /// Reset the sound system by stopping any currently playing sounds.
    /// # Returns
    /// - `true` - If the sound API successfully stopped all sounds, indicating that sounds can be played without issue.
    /// - `false` - If the sound API failed to stop sounds, indicating a potential issue with the sound system.
    pub(crate) fn reset() -> bool {
        // Passing NULL tells PlaySound to purge the current queue.
        PlaySound(Snd::Stop).is_ok()
    }

    /// Toggle the sound enabled state in the preferences and reset the sound system.
    /// If sound is being disabled, this will stop any currently playing sounds.
    /// # Arguments
    /// - `sound_enabled` - A mutable reference to the current sound enabled state in the preferences.
    pub(crate) fn toggle(sound_enabled: &mut bool) {
        *sound_enabled = if *sound_enabled {
            // Stop any currently playing sounds and disable sound
            Self::reset();
            false
        } else {
            // Enable sound if the sound system is responsive
            Self::reset()
        };
    }
}
