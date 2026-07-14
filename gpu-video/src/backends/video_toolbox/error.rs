use objc2_core_media as cm;
use objc2_core_video as cv;
use objc2_video_toolbox as vt;

use crate::{
    VideoBackendError, VideoDecoderError, VideoDeviceInitError,
    parser::{h264::H264ParserError, reference_manager::ReferenceManagementError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OSStatusError {
    // VideoToolbox general
    #[error("VT: property not supported")]
    VTPropertyNotSupported,
    #[error("VT: property is read-only")]
    VTPropertyReadOnly,
    #[error("VT: invalid parameter")]
    VTParameter,
    #[error("VT: invalid session")]
    VTInvalidSession,
    #[error("VT: allocation failed")]
    VTAllocationFailed,
    #[error("VT: could not find video decoder")]
    VTCouldNotFindVideoDecoder,
    #[error("VT: could not create instance")]
    VTCouldNotCreateInstance,
    #[error("VT: format description change not supported")]
    VTFormatDescriptionChangeNotSupported,

    // VideoToolbox decoder
    #[error("VT: decoder received bad data")]
    VTVideoDecoderBadData,
    #[error("VT: decoder unsupported data format")]
    VTVideoDecoderUnsupportedDataFormat,
    #[error("VT: decoder malfunction")]
    VTVideoDecoderMalfunction,
    #[error("VT: decoder not available now")]
    VTVideoDecoderNotAvailableNow,
    #[error("VT: decoder authorization error")]
    VTVideoDecoderAuthorization,
    #[error("VT: decoder removed")]
    VTVideoDecoderRemoved,
    #[error("VT: session malfunction")]
    VTSessionMalfunction,
    #[error("VT: decoder reference missing")]
    VTVideoDecoderReferenceMissing,
    #[error("VT: decoder callback messaging error")]
    VTVideoDecoderCallbackMessaging,
    #[error("VT: decoder unknown error")]
    VTVideoDecoderUnknown,

    // VideoToolbox encoder
    #[error("VT: could not find video encoder")]
    VTCouldNotFindVideoEncoder,
    #[error("VT: encoder malfunction")]
    VTVideoEncoderMalfunction,
    #[error("VT: encoder not available now")]
    VTVideoEncoderNotAvailableNow,
    #[error("VT: encoder authorization error")]
    VTVideoEncoderAuthorization,
    #[error("VT: encoder needs Rosetta")]
    VTVideoEncoderNeedsRosetta,
    #[error("VT: encoder auto white balance not locked")]
    VTVideoEncoderAutoWhiteBalanceNotLocked,

    // CMBlockBuffer
    #[error("CMBlockBuffer: structure allocation failed")]
    CMBlockBufferStructureAllocationFailed,
    #[error("CMBlockBuffer: block allocation failed")]
    CMBlockBufferBlockAllocationFailed,
    #[error("CMBlockBuffer: bad custom block source")]
    CMBlockBufferBadCustomBlockSource,
    #[error("CMBlockBuffer: bad offset parameter")]
    CMBlockBufferBadOffsetParameter,
    #[error("CMBlockBuffer: bad length parameter")]
    CMBlockBufferBadLengthParameter,
    #[error("CMBlockBuffer: bad pointer parameter")]
    CMBlockBufferBadPointerParameter,
    #[error("CMBlockBuffer: empty block buffer")]
    CMBlockBufferEmptyBBuf,
    #[error("CMBlockBuffer: unallocated block")]
    CMBlockBufferUnallocatedBlock,
    #[error("CMBlockBuffer: insufficient space")]
    CMBlockBufferInsufficientSpace,

    // CMFormatDescription
    #[error("CMFormatDescription: invalid parameter")]
    CMFormatDescriptionInvalidParameter,
    #[error("CMFormatDescription: allocation failed")]
    CMFormatDescriptionAllocationFailed,

    // CMSampleBuffer
    #[error("CMSampleBuffer: allocation failed")]
    CMSampleBufferAllocationFailed,
    #[error("CMSampleBuffer: required parameter missing")]
    CMSampleBufferRequiredParameterMissing,
    #[error("CMSampleBuffer: already has data buffer")]
    CMSampleBufferAlreadyHasDataBuffer,
    #[error("CMSampleBuffer: buffer not ready")]
    CMSampleBufferBufferNotReady,
    #[error("CMSampleBuffer: sample index out of range")]
    CMSampleBufferSampleIndexOutOfRange,
    #[error("CMSampleBuffer: buffer has no sample sizes")]
    CMSampleBufferBufferHasNoSampleSizes,
    #[error("CMSampleBuffer: buffer has no sample timing info")]
    CMSampleBufferBufferHasNoSampleTimingInfo,
    #[error("CMSampleBuffer: array too small")]
    CMSampleBufferArrayTooSmall,
    #[error("CMSampleBuffer: invalid entry count")]
    CMSampleBufferInvalidEntryCount,
    #[error("CMSampleBuffer: cannot subdivide")]
    CMSampleBufferCannotSubdivide,
    #[error("CMSampleBuffer: sample timing info invalid")]
    CMSampleBufferSampleTimingInfoInvalid,
    #[error("CMSampleBuffer: invalid media type for operation")]
    CMSampleBufferInvalidMediaTypeForOperation,
    #[error("CMSampleBuffer: invalid sample data")]
    CMSampleBufferInvalidSampleData,
    #[error("CMSampleBuffer: invalid media format")]
    CMSampleBufferInvalidMediaFormat,
    #[error("CMSampleBuffer: invalidated")]
    CMSampleBufferInvalidated,
    #[error("CMSampleBuffer: data failed")]
    CMSampleBufferDataFailed,
    #[error("CMSampleBuffer: data canceled")]
    CMSampleBufferDataCanceled,

    // CVReturn (pixel buffer & Metal texture cache)
    #[error("CVReturn: invalid argument")]
    CVInvalidArgument,
    #[error("CVReturn: allocation failed")]
    CVAllocationFailed,
    #[error("CVReturn: unsupported")]
    CVUnsupported,
    #[error("CVReturn: invalid pixel format")]
    CVInvalidPixelFormat,
    #[error("CVReturn: invalid size")]
    CVInvalidSize,
    #[error("CVReturn: invalid pixel buffer attributes")]
    CVInvalidPixelBufferAttributes,
    #[error("CVReturn: pixel buffer not Metal compatible")]
    CVPixelBufferNotMetalCompatible,
    #[error("CVReturn: retry")]
    CVRetry,

    #[error("unknown OSStatus error: {0}")]
    Unknown(i32),
}

impl OSStatusError {
    pub(crate) fn from_code(code: i32) -> Self {
        match code {
            vt::kVTPropertyNotSupportedErr => Self::VTPropertyNotSupported,
            vt::kVTPropertyReadOnlyErr => Self::VTPropertyReadOnly,
            vt::kVTParameterErr => Self::VTParameter,
            vt::kVTInvalidSessionErr => Self::VTInvalidSession,
            vt::kVTAllocationFailedErr => Self::VTAllocationFailed,
            vt::kVTCouldNotFindVideoDecoderErr => Self::VTCouldNotFindVideoDecoder,
            vt::kVTCouldNotCreateInstanceErr => Self::VTCouldNotCreateInstance,
            vt::kVTFormatDescriptionChangeNotSupportedErr => {
                Self::VTFormatDescriptionChangeNotSupported
            }

            vt::kVTVideoDecoderBadDataErr => Self::VTVideoDecoderBadData,
            vt::kVTVideoDecoderUnsupportedDataFormatErr => {
                Self::VTVideoDecoderUnsupportedDataFormat
            }
            vt::kVTVideoDecoderMalfunctionErr => Self::VTVideoDecoderMalfunction,
            vt::kVTVideoDecoderNotAvailableNowErr => Self::VTVideoDecoderNotAvailableNow,
            vt::kVTVideoDecoderAuthorizationErr => Self::VTVideoDecoderAuthorization,
            vt::kVTVideoDecoderRemovedErr => Self::VTVideoDecoderRemoved,
            vt::kVTSessionMalfunctionErr => Self::VTSessionMalfunction,
            vt::kVTVideoDecoderReferenceMissingErr => Self::VTVideoDecoderReferenceMissing,
            vt::kVTVideoDecoderCallbackMessagingErr => Self::VTVideoDecoderCallbackMessaging,
            vt::kVTVideoDecoderUnknownErr => Self::VTVideoDecoderUnknown,

            vt::kVTCouldNotFindVideoEncoderErr => Self::VTCouldNotFindVideoEncoder,
            vt::kVTVideoEncoderMalfunctionErr => Self::VTVideoEncoderMalfunction,
            vt::kVTVideoEncoderNotAvailableNowErr => Self::VTVideoEncoderNotAvailableNow,
            vt::kVTVideoEncoderAuthorizationErr => Self::VTVideoEncoderAuthorization,
            vt::kVTVideoEncoderNeedsRosettaErr => Self::VTVideoEncoderNeedsRosetta,
            vt::kVTVideoEncoderAutoWhiteBalanceNotLockedErr => {
                Self::VTVideoEncoderAutoWhiteBalanceNotLocked
            }

            cm::kCMBlockBufferStructureAllocationFailedErr => {
                Self::CMBlockBufferStructureAllocationFailed
            }
            cm::kCMBlockBufferBlockAllocationFailedErr => Self::CMBlockBufferBlockAllocationFailed,
            cm::kCMBlockBufferBadCustomBlockSourceErr => Self::CMBlockBufferBadCustomBlockSource,
            cm::kCMBlockBufferBadOffsetParameterErr => Self::CMBlockBufferBadOffsetParameter,
            cm::kCMBlockBufferBadLengthParameterErr => Self::CMBlockBufferBadLengthParameter,
            cm::kCMBlockBufferBadPointerParameterErr => Self::CMBlockBufferBadPointerParameter,
            cm::kCMBlockBufferEmptyBBufErr => Self::CMBlockBufferEmptyBBuf,
            cm::kCMBlockBufferUnallocatedBlockErr => Self::CMBlockBufferUnallocatedBlock,
            cm::kCMBlockBufferInsufficientSpaceErr => Self::CMBlockBufferInsufficientSpace,

            cm::kCMFormatDescriptionError_InvalidParameter => {
                Self::CMFormatDescriptionInvalidParameter
            }
            cm::kCMFormatDescriptionError_AllocationFailed => {
                Self::CMFormatDescriptionAllocationFailed
            }

            cm::kCMSampleBufferError_AllocationFailed => Self::CMSampleBufferAllocationFailed,
            cm::kCMSampleBufferError_RequiredParameterMissing => {
                Self::CMSampleBufferRequiredParameterMissing
            }
            cm::kCMSampleBufferError_AlreadyHasDataBuffer => {
                Self::CMSampleBufferAlreadyHasDataBuffer
            }
            cm::kCMSampleBufferError_BufferNotReady => Self::CMSampleBufferBufferNotReady,
            cm::kCMSampleBufferError_SampleIndexOutOfRange => {
                Self::CMSampleBufferSampleIndexOutOfRange
            }
            cm::kCMSampleBufferError_BufferHasNoSampleSizes => {
                Self::CMSampleBufferBufferHasNoSampleSizes
            }
            cm::kCMSampleBufferError_BufferHasNoSampleTimingInfo => {
                Self::CMSampleBufferBufferHasNoSampleTimingInfo
            }
            cm::kCMSampleBufferError_ArrayTooSmall => Self::CMSampleBufferArrayTooSmall,
            cm::kCMSampleBufferError_InvalidEntryCount => Self::CMSampleBufferInvalidEntryCount,
            cm::kCMSampleBufferError_CannotSubdivide => Self::CMSampleBufferCannotSubdivide,
            cm::kCMSampleBufferError_SampleTimingInfoInvalid => {
                Self::CMSampleBufferSampleTimingInfoInvalid
            }
            cm::kCMSampleBufferError_InvalidMediaTypeForOperation => {
                Self::CMSampleBufferInvalidMediaTypeForOperation
            }
            cm::kCMSampleBufferError_InvalidSampleData => Self::CMSampleBufferInvalidSampleData,
            cm::kCMSampleBufferError_InvalidMediaFormat => Self::CMSampleBufferInvalidMediaFormat,
            cm::kCMSampleBufferError_Invalidated => Self::CMSampleBufferInvalidated,
            cm::kCMSampleBufferError_DataFailed => Self::CMSampleBufferDataFailed,
            cm::kCMSampleBufferError_DataCanceled => Self::CMSampleBufferDataCanceled,

            cv::kCVReturnInvalidArgument => Self::CVInvalidArgument,
            cv::kCVReturnAllocationFailed => Self::CVAllocationFailed,
            cv::kCVReturnUnsupported => Self::CVUnsupported,
            cv::kCVReturnInvalidPixelFormat => Self::CVInvalidPixelFormat,
            cv::kCVReturnInvalidSize => Self::CVInvalidSize,
            cv::kCVReturnInvalidPixelBufferAttributes => Self::CVInvalidPixelBufferAttributes,
            cv::kCVReturnPixelBufferNotMetalCompatible => Self::CVPixelBufferNotMetalCompatible,
            cv::kCVReturnRetry => Self::CVRetry,

            other => Self::Unknown(other),
        }
    }
}

pub(crate) trait OSStatusExt {
    fn osstatus(self) -> Result<(), OSStatusError>;
}

impl OSStatusExt for i32 {
    fn osstatus(self) -> Result<(), OSStatusError> {
        if self == 0 {
            Ok(())
        } else {
            Err(OSStatusError::from_code(self))
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VTDecoderError {
    #[error(transparent)]
    OSStatus(#[from] OSStatusError),

    #[error("H264 parser error: {0}")]
    ParserError(#[from] H264ParserError),

    #[error("Reference management error: {0}")]
    ReferenceManagementError(#[from] ReferenceManagementError),

    #[error("Trying to decode with no session active (probably before receiving first SPS)")]
    NoSession,

    #[error("Invalid input data: {0}")]
    InvalidInputData(String),

    #[error("Failed to extract Metal texture from CVMetalTexture")]
    MetalTextureExtractionFailed,

    #[error("VideoToolbox decoder produced no output (callback was not called)")]
    NoDecoderOutput,

    #[error("VideoToolbox decoder returned success with no image and no FrameDropped flag")]
    UnexpectedNullImage,
}

impl From<VTDecoderError> for VideoDecoderError {
    fn from(err: VTDecoderError) -> Self {
        VideoDecoderError::BackendError(VideoBackendError {
            message: err.to_string(),
            source: Box::new(err),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VTInitError {
    #[error(transparent)]
    OSStatus(#[from] OSStatusError),

    #[error("wgpu device is not backed by Metal")]
    NotMetalBackend,

    #[cfg(feature = "wgpu")]
    #[error(transparent)]
    Wgpu(#[from] crate::WgpuInitError),
}

impl From<VTInitError> for VideoDeviceInitError {
    fn from(err: VTInitError) -> Self {
        VideoDeviceInitError::BackendError(VideoBackendError {
            message: err.to_string(),
            source: Box::new(err),
        })
    }
}

impl From<VTInitError> for VideoDecoderError {
    fn from(err: VTInitError) -> Self {
        VideoDecoderError::BackendError(VideoBackendError {
            message: err.to_string(),
            source: Box::new(err),
        })
    }
}
