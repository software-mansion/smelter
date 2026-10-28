use std::{collections::HashMap, time::Duration};

use tracing::warn;

use crate::{
    Frame, FrameSet, InputId, OutputId,
    state::{RenderCtx, node::RenderNode, render_graph::RenderGraph},
};

use super::{input_texture::InputTexture, node_texture::NodeTexture};

pub(super) fn populate_inputs(
    ctx: &RenderCtx,
    scene: &mut RenderGraph,
    mut frame_set: FrameSet<InputId>,
) {
    for (input_id, (_node_texture, input_textures)) in &mut scene.inputs {
        let Some(frame) = frame_set.frames.remove(input_id) else {
            input_textures.clear();
            continue;
        };
        input_textures.upload(ctx.wgpu_ctx, frame);
    }

    let mut encoder = ctx.wgpu_ctx.create_command_encoder("Input conversion");
    for (node_texture, input_textures) in scene.inputs.values_mut() {
        input_textures.convert_to_node_texture(ctx.wgpu_ctx, node_texture, &mut encoder);
    }
    ctx.wgpu_ctx.queue.submit(Some(encoder.finish()));
}

pub(super) fn read_outputs(
    ctx: &RenderCtx,
    render_graph: &mut RenderGraph,
    pts: Duration,
) -> HashMap<OutputId, Frame> {
    let RenderGraph {
        render_trees,
        outputs,
        inputs,
    } = render_graph;
    let partial_frames: Vec<_> = outputs
        .iter_mut()
        .map(|(output_id, output)| {
            // `None` if the root is empty, or if a linked output does not render its own scene.
            let node_texture = render_trees
                .get(&output.render_tree_id)
                .and_then(|root| root.output_texture(inputs).state());
            let partial_frame = output.output_texture.read(ctx, node_texture, pts);
            (output_id, partial_frame)
        })
        .collect();

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

pub(super) fn run_transforms(ctx: &mut RenderCtx, scene: &mut RenderGraph, pts: Duration) {
    for root in scene.render_trees.values_mut() {
        render_node(ctx, &scene.inputs, pts, root);
    }
}

pub(super) fn render_node(
    ctx: &mut RenderCtx,
    inputs: &HashMap<InputId, (NodeTexture, InputTexture)>,
    pts: Duration,
    node: &mut RenderNode,
) {
    for child_node in node.children.iter_mut() {
        render_node(ctx, inputs, pts, child_node);
    }

    let input_textures: Vec<_> = node
        .children
        .iter()
        .map(|node| node.output_texture(inputs))
        .collect();
    node.renderer
        .render(ctx, &input_textures, &mut node.output, pts);
}
