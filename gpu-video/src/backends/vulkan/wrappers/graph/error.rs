use ash::vk;

use crate::backends::vulkan::{
    VulkanCommonError, VulkanDecoderError, vulkan_encoder::VulkanEncoderError,
};

#[cfg(feature = "transcoder")]
use crate::backends::vulkan::VulkanTranscoderError;

use super::{ExecError, ResourceUsage, Role, Sharing, Use};

#[derive(Debug, thiserror::Error)]
pub(crate) enum GraphError {
    #[error("{usage:?} needs one of {needed:?}, the {role:?} family reports {has:?}")]
    StageNotSupportedOnQueue {
        usage: Use,
        role: Role,
        needed: vk::QueueFlags,
        has: vk::QueueFlags,
    },
    #[error("{usage:?} requires usage {required:?}, the resource has {has:?}")]
    UsageMissing {
        usage: Use,
        required: ResourceUsage,
        has: ResourceUsage,
    },
    #[error("{usage:?} cannot be used as a {direction}")]
    UseDirection { usage: Use, direction: &'static str },
    #[error("family {family} is not allowed by the resource's sharing {sharing:?}")]
    FamilyNotShared { family: u32, sharing: Sharing },
    #[error("subresource declared twice in one pass with {first:?} and {second:?}")]
    ConflictingAccess { first: Use, second: Use },
    #[error("subresource declared twice in one pass with different views")]
    ConflictingView,
    #[error("the subresource already has version {latest} in this graph; pass the token")]
    UseProducedToken { latest: u32 },
    #[error("the token is version {token}, the subresource is at version {latest}")]
    StaleVersion { token: u32, latest: u32 },
    #[error("the token belongs to another graph")]
    ForeignToken,
    #[error("the token's pass was never finished")]
    UnknownPass,
    #[error("the image was exported to wgpu and may not be used by gpu-video any more")]
    ImageExported,
    #[error("the image is lent to wgpu; bring it back with import_from_wgpu first")]
    ImageLentToWgpu,
    #[error("import: the image is not lent to wgpu")]
    ImageNotLent,
    #[error("import: the texture does not wrap this image")]
    TextureImageMismatch,
    #[error("import: the range must cover every layer")]
    PartialImport,
    #[error("a graph may import at most one image")]
    SecondImport,
    #[error("export wants layout {want:?} but the producer leaves {have:?}")]
    ExportLayoutMismatch {
        have: vk::ImageLayout,
        want: vk::ImageLayout,
    },
    #[error("the wgpu device or queue does not wrap this VkDevice")]
    WrongWgpuDevice,
    #[error("Role::Wgpu cannot be bound with bind(); use bind_wgpu()")]
    NoCommandPool,
    #[error("the session belongs to family {session} but the pass runs on family {pass}")]
    SessionFamilyMismatch { session: u32, pass: u32 },
    #[error("the session is declared by another open graph")]
    SessionInOpenGraph,
    #[error("a fresh or poisoned session needs a pass that calls Recorder::control_reset")]
    SessionNotReset,
    #[error("control_reset: flags lack RESET")]
    ControlWithoutReset,
    #[error("the pass declared no session")]
    NoSessionDeclared,
    #[error("control_reset must follow begin_coding and precede the coding operation")]
    ControlOutOfPlace,
    #[error("begin_coding called twice in one pass")]
    BeginCodingTwice,
    #[error("no coding scope is open")]
    ScopeNotOpen,
    #[error("the pass opened a coding scope and did not end it")]
    ScopeLeftOpen,
    #[error("with_view: the view belongs to another image")]
    ViewImageMismatch,
    #[error("import: the lent image has an unreached record on another timeline")]
    ImageNotRetired,
    #[error("the pass recorded no body")]
    EmptyPass,
    #[error("the graph already has a completion callback")]
    CallbackAlreadySet,
    #[error("a pass may record only once")]
    RecordAlreadySet,
    #[error("the pass did not declare this range in this role")]
    UndeclaredInPass,
    #[error("the range carries no view; build it with slot() or with_view()")]
    NoViewForRange,
    #[error("a graph of this window is still open")]
    GraphAlreadyOpen,
    #[error("this call would block on the executor's own waiter thread")]
    CalledOnWaiterThread,
    #[error("no queue is available for role {0:?}")]
    NoQueueForRole(Role),
    #[error("this encoder has no transfer binding; it cannot upload bytes")]
    NoTransferBinding,

    #[error("pass {0} was never finished")]
    UnfinishedPass(&'static str),
    #[error("host reads are declared but no completion callback is set")]
    HostReadNeverConsumed,
    #[error("a host read names a version that a later pass overwrote")]
    StaleHostRead,
    #[error("EXCLUSIVE resource owned by family {owner} used on family {family} without discard")]
    OwnershipTransferRequired { owner: u32, family: u32 },
    #[error("a read of a subresource that was never written (layout UNDEFINED)")]
    ReadOfUninitialized,
    #[error("the graph came from another window")]
    WrongWindow,
    #[error("the completion belongs to another graph")]
    WrongGraph,

    #[error("Vulkan error: {0}")]
    Vk(#[from] vk::Result),
    #[error("recording failed: {0}")]
    Record(#[from] VulkanCommonError),
    #[cfg(feature = "wgpu")]
    #[error("wgpu rejected the import batch: {0}")]
    Wgpu(wgpu::Error),
    #[cfg(feature = "wgpu")]
    #[error("wgpu's queue did not accept the import (device destroyed or lost)")]
    WgpuQueueRejected,
    #[error("a later pass of this graph failed to submit; the prefix completed")]
    PartialSubmit,
    #[error("the device was lost")]
    DeviceLost,
    #[error("the executor's waiter thread is gone")]
    WaiterThreadDied,
    #[error("a submit panicked under the executor lock; the window is unusable")]
    SubmitPanicked,

    #[error("the host read was not declared in this graph")]
    UndeclaredHostRead,
    #[error("a submission failed with query status {0:?}")]
    SubmissionFailed(vk::QueryResultStatusKHR),
    #[error("timed out waiting for a submission to finish")]
    WaitTimeout,
    #[error("a completion callback panicked earlier")]
    CallbackPanicked,
}

impl From<ExecError> for GraphError {
    fn from(e: ExecError) -> Self {
        match e {
            ExecError::PartialSubmit => GraphError::PartialSubmit,
            ExecError::DeviceLost => GraphError::DeviceLost,
            ExecError::CallbackPanicked => GraphError::CallbackPanicked,
        }
    }
}

impl From<GraphError> for VulkanDecoderError {
    fn from(error: GraphError) -> Self {
        todo!()
    }
}

impl From<GraphError> for VulkanEncoderError {
    fn from(error: GraphError) -> Self {
        todo!()
    }
}

#[cfg(feature = "transcoder")]
impl From<GraphError> for VulkanTranscoderError {
    fn from(error: GraphError) -> Self {
        todo!()
    }
}
