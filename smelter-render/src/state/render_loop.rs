use std::{collections::HashMap, time::Duration};

use tracing::warn;

use crate::{Frame, FrameSet, InputId, OutputId, Resolution, scene::SceneState, state::RenderCtx};

use super::{input_texture::InputTextures, output_texture::OutputTexture};

pub(super) struct OutputRenderer {
    /// Id of an output that defines the rendered scene. Own id, or id of the primary output
    /// for linked outputs.
    pub(super) scene_output_id: OutputId,
    pub(super) output_texture: OutputTexture,
}

pub(super) fn populate_inputs(
    ctx: &mut RenderCtx,
    inputs: &mut InputTextures,
    mut frame_set: FrameSet<InputId>,
) {
    for (input_id, (_node_texture, input_textures)) in inputs.iter_mut() {
        let Some(frame) = frame_set.frames.remove(input_id) else {
            input_textures.clear();
            continue;
        };
        input_textures.upload(ctx.wgpu_ctx, frame);
    }

    for (node_texture, input_textures) in inputs.values_mut() {
        input_textures.convert_to_node_texture(ctx.wgpu_ctx, node_texture, &mut ctx.encoder);
    }
    ctx.submit_recorded_commands();
}

/// Resolutions of frames from the last `populate_inputs` call. Inputs without a frame
/// are skipped.
pub(super) fn input_resolutions(inputs: &InputTextures) -> HashMap<InputId, Resolution> {
    inputs
        .iter()
        .filter_map(|(input_id, (node_texture, _))| {
            Some((input_id.clone(), node_texture.resolution()?))
        })
        .collect()
}

pub(super) fn read_outputs(
    ctx: &mut RenderCtx,
    outputs: &mut HashMap<OutputId, OutputRenderer>,
    scene: &SceneState,
    inputs: &InputTextures,
    pts: Duration,
) -> HashMap<OutputId, Frame> {
    let partial_frames: Vec<_> = outputs
        .iter_mut()
        .map(|(output_id, output)| {
            // `None` if the root is empty, or if a linked output does not render its own scene.
            let node_texture = scene
                .output_texture(&output.scene_output_id, inputs)
                .and_then(|texture| texture.state());
            let partial_frame = output.output_texture.read(ctx, node_texture, pts);
            (output_id, partial_frame)
        })
        .collect();
    ctx.submit_recorded_commands();

    while let Err(wgpu::PollError::Timeout) = ctx
        .wgpu_ctx
        .device
        .poll(wgpu::PollType::wait_indefinitely())
    {
        warn!("Device poll failed.")
    }

    partial_frames
        .into_iter()
        .filter_map(|(output_id, partial_frame)| Some((output_id.clone(), partial_frame.finish()?)))
        .collect()
}

pub(super) fn render_scenes(
    ctx: &mut RenderCtx,
    scene: &mut SceneState,
    inputs: &InputTextures,
    pts: Duration,
) {
    scene.render(ctx, inputs, pts);
    ctx.submit_recorded_commands();
}
