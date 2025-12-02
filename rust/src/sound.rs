use core::ffi::c_int;
use core::ptr::null;

use windows_sys::Win32::Foundation::HINSTANCE;
use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_PURGE, SND_RESOURCE};

use crate::pref::{Preferences, FSOUND_OFF, FSOUND_ON};

// Tune identifiers passed in from the legacy C code.
const TUNE_TICK: c_int = 1;
const TUNE_WINGAME: c_int = 2;
const TUNE_LOSEGAME: c_int = 3;

// Resource IDs for the embedded .wav assets.
const ID_TUNE_TICK: u16 = 432;
const ID_TUNE_WON: u16 = 433;
const ID_TUNE_LOST: u16 = 434;

extern "C" {
    static mut hInst: HINSTANCE;
}

#[no_mangle]
pub extern "C" fn FInitTunes() -> c_int {
    // Attempt to stop any playing sounds; if the API fails we assume the
    // machine cannot play audio and disable sound effects in preferences.
    if stop_all_sounds() {
        FSOUND_ON
    } else {
        FSOUND_OFF
    }
}

#[no_mangle]
pub extern "C" fn EndTunes() {
    // When exiting, purge the playback queue if the feature is active.
    if sound_enabled() {
        let _ = stop_all_sounds();
    }
}

#[no_mangle]
pub extern "C" fn PlayTune(tune: c_int) {
    // Honor the user's preference before attempting to play any sound.
    if !sound_enabled() {
        return;
    }

    // Map the logical tune ID to a bundled resource and play it.
    match tune {
        TUNE_TICK => play_resource_sound(ID_TUNE_TICK),
        TUNE_WINGAME => play_resource_sound(ID_TUNE_WON),
        TUNE_LOSEGAME => play_resource_sound(ID_TUNE_LOST),
        _ => {}
    }
}

fn sound_enabled() -> bool {
    unsafe { Preferences.fSound == FSOUND_ON }
}

fn stop_all_sounds() -> bool {
    // Passing NULL tells PlaySound to purge the current queue.
    unsafe { PlaySoundW(null(), 0, SND_PURGE) != 0 }
}

fn play_resource_sound(resource_id: u16) {
    let resource_ptr = make_int_resource(resource_id);
    let instance = unsafe { hInst };
    // Playback uses the async flag so the UI thread is never blocked.
    unsafe {
        PlaySoundW(resource_ptr, instance, SND_RESOURCE | SND_ASYNC);
    }
}

fn make_int_resource(resource_id: u16) -> *const u16 {
    resource_id as usize as *const u16
}
