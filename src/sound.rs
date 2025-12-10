/// Quick helpers for the small set of winmm-backed tunes used by the UI.
use core::ptr::{null, null_mut};

use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_PURGE, SND_RESOURCE};

use crate::globals::global_state;
use crate::pref::{FSOUND_OFF, FSOUND_ON};

/// Logical UI tunes that map to embedded wave resources.
pub enum Tune {
    /// Short tick used for timer and click feedback.
    Tick,
    /// Win jingle played after successfully clearing the board.
    WinGame,
    /// Loss sound played after detonating a mine.
    LoseGame,
}

pub fn FInitTunes() -> i32 {
    // Attempt to stop any playing sounds; if the API fails we assume the
    // machine cannot play audio and disable sound effects in preferences.
    if stop_all_sounds() {
        FSOUND_ON
    } else {
        FSOUND_OFF
    }
}

pub fn EndTunes() {
    // Purge the playback queue; callers decide whether sound is enabled.
    let _ = stop_all_sounds();
}

fn stop_all_sounds() -> bool {
    // Passing NULL tells PlaySound to purge the current queue.
    unsafe { PlaySoundW(null(), null_mut(), SND_PURGE) != 0 }
}

/// Play a specific UI tune using the sounds in the resource file
pub fn PlayTune(tune: Tune) {
    let resource_id: u16 = match tune {
        Tune::Tick => 432,
        Tune::WinGame => 433,
        Tune::LoseGame => 434,
    };

    let resource_ptr = make_int_resource(resource_id);
    let instance_ptr = {
        let guard = match global_state().h_inst.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.ptr()
    };
    // Playback uses the async flag so the UI thread is never blocked.
    unsafe {
        PlaySoundW(resource_ptr, instance_ptr, SND_RESOURCE | SND_ASYNC);
    }
}

fn make_int_resource(resource_id: u16) -> *const u16 {
    resource_id as usize as *const u16
}
