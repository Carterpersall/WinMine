// Quick helpers for the small set of winmm-backed tunes used by the UI.
use core::ptr::{null, null_mut};

use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_PURGE, SND_RESOURCE};

use crate::globals::hInst;
use crate::pref::{FSOUND_OFF, FSOUND_ON};
use crate::rtns::Preferences;

// Tune identifiers passed in from the legacy C code.
const TUNE_TICK: i32 = 1;
const TUNE_WINGAME: i32 = 2;
const TUNE_LOSEGAME: i32 = 3;

// Resource IDs for the embedded .wav assets.
const ID_TUNE_TICK: u16 = 432;
const ID_TUNE_WON: u16 = 433;
const ID_TUNE_LOST: u16 = 434;

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
    // When exiting, purge the playback queue if the feature is active.
    if sound_enabled() {
        let _ = stop_all_sounds();
    }
}

pub fn PlayTune(tune: i32) {
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
    unsafe { PlaySoundW(null(), null_mut(), SND_PURGE) != 0 }
}

fn play_resource_sound(resource_id: u16) {
    let resource_ptr = make_int_resource(resource_id);
    let instance = unsafe { hInst.ptr() };
    // Playback uses the async flag so the UI thread is never blocked.
    unsafe {
        PlaySoundW(resource_ptr, instance, SND_RESOURCE | SND_ASYNC);
    }
}

fn make_int_resource(resource_id: u16) -> *const u16 {
    resource_id as usize as *const u16
}
