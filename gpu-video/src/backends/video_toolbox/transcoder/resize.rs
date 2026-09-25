use crate::{
    backends::video_toolbox::error::VTTranscoderError, parameters::ScalingAlgorithm,
    transcoder::TranscoderOutputParameters,
};

use objc2::{AnyThread, rc::Retained, runtime::ProtocolObject};
use objc2_metal as mtl;
use objc2_metal_performance_shaders as mps;

pub(crate) struct Resizer {
    resizer_y: Retained<mps::MPSImageScale>,
    resizer_uv: Retained<mps::MPSImageScale>,
}

// safety: docs say they're safe unless they're not being used simultaneously by two or more
// threads. Send does not allow that to happen.
// You can use multiple MPSKernel objects on multiple threads, as long as only one thread is operating on any particular MPSKernel object at a time.
unsafe impl Send for Resizer {}

impl Resizer {
    pub(crate) fn new(
        params: &TranscoderOutputParameters,
        device: &ProtocolObject<dyn mtl::MTLDevice>,
    ) -> Result<Self, VTTranscoderError> {
        if unsafe { !mps::MPSSupportsMTLDevice(Some(device)) } {
            return Err(VTTranscoderError::MetalPerformanceShadersUnsupported);
        }

        if matches!(params.scaling_algorithm, ScalingAlgorithm::NearestNeighbor) {
            tracing::warn!("Nearest neighbor scaling is not available on Apple; using bilinear");
        }

        Ok(Self {
            resizer_y: create_kernel(params, device),
            resizer_uv: create_kernel(params, device),
        })
    }

    pub(crate) fn encode_y(
        &self,
        buffer: &ProtocolObject<dyn mtl::MTLCommandBuffer>,
        src: &ProtocolObject<dyn mtl::MTLTexture>,
        dst: &ProtocolObject<dyn mtl::MTLTexture>,
    ) {
        unsafe {
            self.resizer_y
                .encodeToCommandBuffer_sourceTexture_destinationTexture(buffer, src, dst)
        };
    }

    pub(crate) fn encode_uv(
        &self,
        buffer: &ProtocolObject<dyn mtl::MTLCommandBuffer>,
        src: &ProtocolObject<dyn mtl::MTLTexture>,
        dst: &ProtocolObject<dyn mtl::MTLTexture>,
    ) {
        unsafe {
            self.resizer_uv
                .encodeToCommandBuffer_sourceTexture_destinationTexture(buffer, src, dst)
        };
    }
}

fn create_kernel(
    params: &TranscoderOutputParameters,
    device: &ProtocolObject<dyn mtl::MTLDevice>,
) -> Retained<mps::MPSImageScale> {
    let resizer = match params.scaling_algorithm {
        ScalingAlgorithm::NearestNeighbor | ScalingAlgorithm::Bilinear => unsafe {
            mps::MPSImageBilinearScale::initWithDevice(mps::MPSImageBilinearScale::alloc(), device)
                .into_super()
        },
        ScalingAlgorithm::Lanczos3 => unsafe {
            mps::MPSImageLanczosScale::initWithDevice(mps::MPSImageLanczosScale::alloc(), device)
                .into_super()
        },
    };

    unsafe { resizer.setEdgeMode(mps::MPSImageEdgeMode::Clamp) };

    resizer
}
