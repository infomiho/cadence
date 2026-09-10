use std::{
    mem::{MaybeUninit, size_of},
    ptr::{NonNull, null},
};

use objc2_core_audio::{
    AudioObjectGetPropertyData, AudioObjectID, AudioObjectIsPropertySettable,
    AudioObjectPropertyAddress, AudioObjectPropertyElement, AudioObjectPropertyScope,
    AudioObjectPropertySelector, AudioObjectSetPropertyData, kAudioDevicePropertyMute,
    kAudioDevicePropertyVolumeScalar, kAudioHardwarePropertyDefaultOutputDevice,
    kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
};

type OSStatus = i32;

const NO_ERROR: OSStatus = 0;
const SYSTEM_OBJECT: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;
/// Probing bound for per-channel volume elements when a device reports more
/// channels than any consumer setup the slider targets.
const MAX_PROBED_ELEMENTS: AudioObjectPropertyElement = 8;

pub(super) fn output_volume() -> Option<f32> {
    let device = default_output_device()?;
    if device_is_muted(device) {
        return Some(0.);
    }
    let addresses = settable_volume_addresses(device);
    let values = addresses
        .iter()
        .filter_map(|address| unsafe { get_property::<f32>(device, address) })
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    let average = values.iter().sum::<f32>() / values.len() as f32;
    Some(average.clamp(0., 1.))
}

pub(super) fn set_output_volume(volume: f32) -> bool {
    let volume = volume.clamp(0., 1.);
    let Some(device) = default_output_device() else {
        return false;
    };
    let mut changed = false;
    for address in settable_volume_addresses(device) {
        changed |= unsafe { set_property(device, &address, &volume) };
    }
    if volume > 0. {
        clear_mute(device);
    }
    changed
}

fn default_output_device() -> Option<AudioObjectID> {
    let device = unsafe {
        get_property::<AudioObjectID>(
            SYSTEM_OBJECT,
            &property_address(
                kAudioHardwarePropertyDefaultOutputDevice,
                kAudioObjectPropertyScopeGlobal,
                MAIN_ELEMENT,
            ),
        )
    }?;
    (device != 0).then_some(device)
}

fn device_is_muted(device: AudioObjectID) -> bool {
    matches!(
        unsafe {
            get_property::<u32>(
                device,
                &property_address(
                    kAudioDevicePropertyMute,
                    kAudioObjectPropertyScopeGlobal,
                    MAIN_ELEMENT,
                ),
            )
        },
        Some(1)
    )
}

/// Devices expose volume either on a master element or per output channel,
/// never both, so both shapes are probed and the settable ones kept.
fn settable_volume_addresses(device: AudioObjectID) -> Vec<AudioObjectPropertyAddress> {
    probe_addresses()
        .into_iter()
        .filter(|address| property_is_settable(device, address))
        .collect()
}

/// Every address that may carry an output volume: the master element plus the
/// per-channel output elements.
fn probe_addresses() -> Vec<AudioObjectPropertyAddress> {
    let mut addresses = vec![property_address(
        kAudioDevicePropertyVolumeScalar,
        kAudioObjectPropertyScopeGlobal,
        MAIN_ELEMENT,
    )];
    addresses.extend((1..=MAX_PROBED_ELEMENTS).map(|element| {
        property_address(
            kAudioDevicePropertyVolumeScalar,
            kAudioObjectPropertyScopeOutput,
            element,
        )
    }));
    addresses
}

const MAIN_ELEMENT: AudioObjectPropertyElement = 0;

fn property_address(
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
    element: AudioObjectPropertyElement,
) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: element,
    }
}

fn property_is_settable(device: AudioObjectID, address: &AudioObjectPropertyAddress) -> bool {
    let mut settable: u8 = 0;
    let status = unsafe {
        AudioObjectIsPropertySettable(device, NonNull::from(address), NonNull::from(&mut settable))
    };
    status == NO_ERROR && settable != 0
}

/// Reads a property whose value is a fixed-size `Copy` type.
unsafe fn get_property<T: Copy>(
    object: AudioObjectID,
    address: &AudioObjectPropertyAddress,
) -> Option<T> {
    let mut value = MaybeUninit::<T>::uninit();
    let mut size = size_of::<T>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(address),
            0,
            null(),
            NonNull::from(&mut size),
            NonNull::from(&mut value).cast(),
        )
    };
    (status == NO_ERROR).then(|| unsafe { value.assume_init() })
}

unsafe fn set_property<T>(
    object: AudioObjectID,
    address: &AudioObjectPropertyAddress,
    value: &T,
) -> bool {
    unsafe {
        AudioObjectSetPropertyData(
            object,
            NonNull::from(address),
            0,
            null(),
            size_of::<T>() as u32,
            NonNull::from(value).cast(),
        ) == NO_ERROR
    }
}

fn clear_mute(device: AudioObjectID) -> bool {
    unsafe {
        set_property(
            device,
            &property_address(
                kAudioDevicePropertyMute,
                kAudioObjectPropertyScopeGlobal,
                MAIN_ELEMENT,
            ),
            &0_u32,
        )
    }
}
