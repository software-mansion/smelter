use std::sync::Arc;

use ash::vk;

use super::{ExportDecl, ImportDecl, TaskError, Timeline, waiter::SharedState};

pub(super) fn run_import(import: &mut ImportDecl) -> Result<(), TaskError> {
    todo!()
}

fn check_import_needs(import: &ImportDecl) -> Result<(), TaskError> {
    todo!()
}

fn stage_previous_wait(import: &ImportDecl) -> Option<u64> {
    todo!()
}

fn submit_transition_batch(import: &ImportDecl) -> Result<(), TaskError> {
    todo!()
}

fn submit_signal_batch(import: &ImportDecl) -> Result<u64, TaskError> {
    todo!()
}

fn signal_never_reached_hal(import: &ImportDecl) -> bool {
    todo!()
}

fn unstage(import: &ImportDecl, wait: bool, signal: bool) {
    todo!()
}

fn commit_import(import: &mut ImportDecl, value: u64) {
    todo!()
}

struct ErrorScopes<'a> {
    device: &'a wgpu::Device,
}

impl<'a> ErrorScopes<'a> {
    fn push(device: &'a wgpu::Device) -> Self {
        todo!()
    }

    fn pop(self) -> Option<wgpu::Error> {
        todo!()
    }
}

pub(super) fn stage_export(
    export: ExportDecl,
    timeline: &Arc<Timeline>,
    value: u64,
    shared: &SharedState,
) {
    todo!()
}

pub(super) fn wgpu_device_handle(device: &wgpu::Device) -> Option<vk::Device> {
    todo!()
}

pub(super) fn wgpu_queue_device_handle(queue: &wgpu::Queue) -> Option<vk::Device> {
    todo!()
}

pub(super) fn texture_raw_image(texture: &wgpu::Texture) -> Option<vk::Image> {
    todo!()
}

pub(super) fn derive_image_layout(
    usage: wgpu::TextureUses,
    format: wgpu::TextureFormat,
) -> vk::ImageLayout {
    todo!()
}
