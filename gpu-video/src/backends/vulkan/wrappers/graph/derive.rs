use std::{collections::HashMap, sync::Arc};

use ash::vk;

use crate::backends::vulkan::wrappers::Device;

use super::{
    Access, AccessRecord, Graph, GraphError, HostReadDecl, Pass, SubresourceKey, SubresourceState,
    Timeline,
};

pub(super) struct Snapshot {
    pub(super) states: HashMap<SubresourceKey, SubresourceState>,
}

impl Snapshot {
    pub(super) fn capture(graph: &Graph) -> Self {
        todo!()
    }

    pub(super) fn apply_commit(&mut self, pass: &Pass, value: u64) {
        todo!()
    }
}

pub(super) struct DerivedPass {
    pub(super) image_barriers: Vec<vk::ImageMemoryBarrier2<'static>>,
    pub(super) host_read_barriers: Vec<vk::BufferMemoryBarrier2<'static>>,
    pub(super) waits: Vec<(Arc<Timeline>, u64)>,
}

pub(super) fn dry_run(graph: &Graph) -> Result<(), GraphError> {
    todo!()
}

fn check_ownership(
    state: &SubresourceState,
    access: &Access,
    family: u32,
) -> Result<(), GraphError> {
    todo!()
}

fn check_initialized(state: &SubresourceState, access: &Access) -> Result<(), GraphError> {
    todo!()
}

fn check_host_reads(graph: &Graph) -> Result<(), GraphError> {
    todo!()
}

pub(super) fn derive_pass(pass: &Pass, host_reads: &[HostReadDecl]) -> DerivedPass {
    todo!()
}

fn needs(state: &SubresourceState, access: &Access) -> Vec<AccessRecord> {
    todo!()
}

fn barrier_masks(
    needs: &[AccessRecord],
    timeline: &Arc<Timeline>,
) -> (vk::PipelineStageFlags2, vk::AccessFlags2) {
    todo!()
}

fn merge_layer_runs(
    barriers: Vec<vk::ImageMemoryBarrier2<'static>>,
) -> Vec<vk::ImageMemoryBarrier2<'static>> {
    todo!()
}

fn merge_waits(waits: Vec<(Arc<Timeline>, u64)>) -> Vec<(Arc<Timeline>, u64)> {
    todo!()
}

pub(super) fn record_prologue(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pass: &Pass,
    derived: &DerivedPass,
) -> Result<(), GraphError> {
    todo!()
}

pub(super) fn record_epilogue(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    derived: &DerivedPass,
) -> Result<(), GraphError> {
    todo!()
}

pub(super) fn submit_pass(
    device: &Device,
    pass: &Pass,
    derived: &DerivedPass,
    prologue: vk::CommandBuffer,
    epilogue: Option<vk::CommandBuffer>,
) -> Result<u64, GraphError> {
    todo!()
}

pub(super) fn commit(pass: &Pass, value: u64) {
    todo!()
}
