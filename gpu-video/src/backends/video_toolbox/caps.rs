use core::ffi::c_void;
use core::ptr::{NonNull, null};

use objc2_core_foundation::{
    CFDictionary, CFRetained, CFString, kCFBooleanTrue, kCFTypeDictionaryKeyCallBacks,
    kCFTypeDictionaryValueCallBacks,
};
use objc2_core_media::{CMVideoCodecType, kCMVideoCodecType_H264, kCMVideoCodecType_HEVC};
use objc2_video_toolbox::{
    VTCopySupportedPropertyDictionaryForEncoder, VTIsHardwareDecodeSupported,
    kVTCompressionPropertyKey_ConstantBitRate,
    kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder,
};

use crate::capabilities::{
    DecodeCapabilities, DecodeH264Capabilities, DecodeH264ProfileCapabilities,
    DecodeH265Capabilities, DecodeH265ProfileCapabilities, EncodeCapabilities,
    EncodeH264Capabilities, EncodeH265Capabilities, EncodeProfileCapabilities,
    RateControlCapabilities,
};

const ASSUMED_MIN_DIMENSION: u32 = 16;
const ASSUMED_MAX_DIMENSION: u32 = 8192;
const ASSUMED_H264_MAX_LEVEL_IDC: u8 = 62;
const ASSUMED_H265_MAX_LEVEL_IDC: u8 = 186;
/// VideoToolbox manages the reference list internally and never exposes a maximum. `0` here means
/// "not reported", it does not mean the encoder cannot use references.
const ASSUMED_ENCODE_MAX_REFERENCES: u32 = 0;
/// "Quality levels" is a Vulkan Video concept. VideoToolbox exposes a continuous `Quality`
/// property instead, so there is nothing discrete to report.
const ASSUMED_ENCODE_QUALITY_LEVELS: u32 = 0;

const ENCODER_PROBE_WIDTH: i32 = 1920;
const ENCODER_PROBE_HEIGHT: i32 = 1080;

pub(crate) fn query_decode_capabilities() -> DecodeCapabilities {
    DecodeCapabilities {
        h264: hardware_decode_supported(kCMVideoCodecType_H264).then(decode_h264_capabilities),
        h265: hardware_decode_supported(kCMVideoCodecType_HEVC).then(decode_h265_capabilities),
    }
}

pub(crate) fn query_encode_capabilities() -> EncodeCapabilities {
    EncodeCapabilities {
        h264: hardware_encoder_properties(kCMVideoCodecType_H264)
            .as_deref()
            .map(encode_h264_capabilities),
        h265: hardware_encoder_properties(kCMVideoCodecType_HEVC)
            .as_deref()
            .map(encode_h265_capabilities),
    }
}

fn hardware_decode_supported(codec: CMVideoCodecType) -> bool {
    unsafe { VTIsHardwareDecodeSupported(codec) }
}

fn decode_h264_capabilities() -> DecodeH264Capabilities {
    // VideoToolbox only reports per-codec support, not per-profile. Apple hardware decoders
    // handle all three H.264 profiles, so we advertise them with the assumed limits above.
    let profile = DecodeH264ProfileCapabilities {
        min_width: ASSUMED_MIN_DIMENSION,
        max_width: ASSUMED_MAX_DIMENSION,
        min_height: ASSUMED_MIN_DIMENSION,
        max_height: ASSUMED_MAX_DIMENSION,
        max_level_idc: ASSUMED_H264_MAX_LEVEL_IDC,
    };

    DecodeH264Capabilities {
        baseline_profile: Some(profile),
        main_profile: Some(profile),
        high_profile: Some(profile),
    }
}

fn decode_h265_capabilities() -> DecodeH265Capabilities {
    DecodeH265Capabilities {
        main_profile: Some(DecodeH265ProfileCapabilities {
            min_width: ASSUMED_MIN_DIMENSION,
            max_width: ASSUMED_MAX_DIMENSION,
            min_height: ASSUMED_MIN_DIMENSION,
            max_height: ASSUMED_MAX_DIMENSION,
            max_level_idc: ASSUMED_H265_MAX_LEVEL_IDC,
        }),
    }
}

fn encode_h264_capabilities(supported_properties: &CFDictionary) -> EncodeH264Capabilities {
    // As with decode, VideoToolbox does not enumerate encode profiles. Apple hardware encoders
    // support baseline/main/high, so we advertise them with the same probed rate-control info.
    let profile = encode_profile_capabilities(supported_properties);
    EncodeH264Capabilities {
        baseline_profile: Some(profile),
        main_profile: Some(profile),
        high_profile: Some(profile),
    }
}

fn encode_h265_capabilities(supported_properties: &CFDictionary) -> EncodeH265Capabilities {
    EncodeH265Capabilities {
        main_profile: Some(encode_profile_capabilities(supported_properties)),
    }
}

fn encode_profile_capabilities(supported_properties: &CFDictionary) -> EncodeProfileCapabilities {
    EncodeProfileCapabilities {
        min_width: ASSUMED_MIN_DIMENSION,
        max_width: ASSUMED_MAX_DIMENSION,
        min_height: ASSUMED_MIN_DIMENSION,
        max_height: ASSUMED_MAX_DIMENSION,
        rate_control: RateControlCapabilities {
            vbr_supported: true,
            cbr_supported: supports_property(supported_properties, unsafe {
                kVTCompressionPropertyKey_ConstantBitRate
            }),
        },
        max_references: ASSUMED_ENCODE_MAX_REFERENCES,
        quality_levels: ASSUMED_ENCODE_QUALITY_LEVELS,
    }
}

/// Returns the supported-property dictionary for a *hardware* encoder of `codec`, or `None` if the
/// system has no hardware encoder for it.
fn hardware_encoder_properties(codec: CMVideoCodecType) -> Option<CFRetained<CFDictionary>> {
    let specification = require_hardware_encoder_specification();

    let mut encoder_id: *const CFString = null();
    let mut supported_properties: *const CFDictionary = null();

    // SAFETY: the out-pointers are valid and the specification is a valid `CFDictionary`.
    let status = unsafe {
        VTCopySupportedPropertyDictionaryForEncoder(
            ENCODER_PROBE_WIDTH,
            ENCODER_PROBE_HEIGHT,
            codec,
            Some(&specification),
            &mut encoder_id,
            &mut supported_properties,
        )
    };

    // `encoder_id` follows the Copy rule and is owned by us; release it (we only need the props).
    if let Some(encoder_id) = NonNull::new(encoder_id.cast_mut()) {
        // SAFETY: on success VideoToolbox handed us a +1 retained `CFString`.
        drop(unsafe { CFRetained::from_raw(encoder_id) });
    }

    if status != 0 {
        return None;
    }

    let supported_properties = NonNull::new(supported_properties.cast_mut())?;
    // SAFETY: on success VideoToolbox handed us a +1 retained `CFDictionary`.
    Some(unsafe { CFRetained::from_raw(supported_properties) })
}

/// Builds `{ RequireHardwareAcceleratedVideoEncoder: true }` so probing rejects the software
/// fallback encoders and only reports genuine hardware capabilities.
fn require_hardware_encoder_specification() -> CFRetained<CFDictionary> {
    // SAFETY: extern statics holding framework-owned constants.
    let key = unsafe { kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder };
    let value = unsafe { kCFBooleanTrue }.expect("kCFBooleanTrue is always available");

    let mut keys: [*const c_void; 1] = [(key as *const CFString).cast()];
    let mut values: [*const c_void; 1] = [(value as *const _ as *const c_void)];

    // SAFETY: `keys`/`values` point to one entry each; the CoreFoundation callbacks correctly
    // retain/release the `CFString`/`CFBoolean` we pass in.
    unsafe {
        CFDictionary::new(
            None,
            keys.as_mut_ptr(),
            values.as_mut_ptr(),
            1,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
    }
    .expect("creating a single-entry CFDictionary cannot fail")
}

fn supports_property(dictionary: &CFDictionary, key: &CFString) -> bool {
    // SAFETY: `key` is a valid `CFString` pointer for the lifetime of the call.
    unsafe { dictionary.contains_ptr_key((key as *const CFString).cast()) }
}
