mod decoder;
mod encoder;
mod parameter_sets;

pub use decoder::{VaapiH264DecoderError, WgpuTexturesDecoder};
pub use encoder::{
    H264EncoderConfig, H264EncoderRateControl, VaapiH264EncoderError, WgpuTexturesEncoderH264,
};

use crate::vaapi::display::open_display;
use libva::{Display, VAEntrypoint, VAProfile};

pub fn supports_encoding(adapter_info: Option<&wgpu::AdapterInfo>) -> bool {
    let Ok(display) = open_display(adapter_info) else {
        return false;
    };

    profile_supports_entrypoint(
        &display,
        VAProfile::VAProfileH264Main,
        &[
            VAEntrypoint::VAEntrypointEncSliceLP,
            VAEntrypoint::VAEntrypointEncSlice,
        ],
    )
}

pub fn supports_decoding(adapter_info: Option<&wgpu::AdapterInfo>) -> bool {
    let Ok(display) = open_display(adapter_info) else {
        return false;
    };

    [
        VAProfile::VAProfileH264ConstrainedBaseline,
        VAProfile::VAProfileH264Main,
        VAProfile::VAProfileH264High,
    ]
    .into_iter()
    .any(|profile| {
        profile_supports_entrypoint(&display, profile, &[VAEntrypoint::VAEntrypointVLD])
    })
}

fn profile_supports_entrypoint(
    display: &Display,
    profile: VAProfile::Type,
    entrypoints: &[VAEntrypoint::Type],
) -> bool {
    display
        .query_config_entrypoints(profile)
        .is_ok_and(|available| entrypoints.iter().any(|entrypoint| available.contains(entrypoint)))
}
