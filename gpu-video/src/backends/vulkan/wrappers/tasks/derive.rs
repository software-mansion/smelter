use std::{collections::HashMap, sync::Arc};

use ash::vk;

use crate::backends::vulkan::wrappers::Device;

use super::{
    Access, AccessRecord, HostReadDecl, Step, SubresourceKey, SubresourceState, Task, TaskError,
    Timeline,
};

pub(super) struct Snapshot {
    pub(super) states: HashMap<SubresourceKey, SubresourceState>,
}

impl Snapshot {
    pub(super) fn capture(task: &Task) -> Self {
        todo!()
    }

    pub(super) fn apply_commit(&mut self, step: &Step, value: u64) {
        todo!()
    }
}

// TODO: change the name
pub(super) struct DerivedStep {
    pub(super) image_barriers: Vec<vk::ImageMemoryBarrier2<'static>>,
    pub(super) host_read_barriers: Vec<vk::BufferMemoryBarrier2<'static>>,
    pub(super) waits: Vec<(Arc<Timeline>, u64)>,
}

pub(super) fn dry_run(task: &Task) -> Result<(), TaskError> {
    todo!()
}

fn check_ownership(
    state: &SubresourceState,
    access: &Access,
    family: u32,
) -> Result<(), TaskError> {
    todo!()
}

fn check_initialized(state: &SubresourceState, access: &Access) -> Result<(), TaskError> {
    todo!()
}

fn check_host_reads(task: &Task) -> Result<(), TaskError> {
    todo!()
}

pub(super) fn derive_step(step: &Step, host_reads: &[HostReadDecl]) -> DerivedStep {
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
    step: &Step,
    derived: &DerivedStep,
) -> Result<(), TaskError> {
    todo!()
}

pub(super) fn record_epilogue(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    derived: &DerivedStep,
) -> Result<(), TaskError> {
    todo!()
}

pub(super) fn submit_step(
    device: &Device,
    step: &Step,
    derived: &DerivedStep,
    prologue: vk::CommandBuffer,
    epilogue: Option<vk::CommandBuffer>,
) -> Result<u64, TaskError> {
    todo!()
}

pub(super) fn commit(step: &Step, value: u64) {
    todo!()
}
