use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use rtmp::{flv::VideoCodec as FlvVideoCodec, RtmpConnection, RtmpError, RtmpMediaData, RtmpServer, ServerConfig};
use tracing::{error, info, warn};

use super::state::RtmpInputsState;

use crate::{
    pipeline::{
        decoder::{
            decoder_thread_video::{VideoDecoderThread, VideoDecoderThreadOptions},
            ffmpeg_h264, vulkan_h264,
        },
        utils::{H264AvcDecoderConfig, H264AvccToAnnexB},
    },
    prelude::*,
    thread_utils::InitializableThread,
};

pub struct RtmpPipelineState {
    pub port: u16,
    pub inputs: RtmpInputsState,
}

impl RtmpPipelineState {
    pub fn new(port: u16) -> Arc<Self> {
        Arc::new(Self {
            port,
            inputs: RtmpInputsState::default(),
        })
    }
}

pub fn spawn_rtmp_server(
    state: &RtmpPipelineState,
) -> Result<Arc<Mutex<RtmpServer>>, InitPipelineError> {
    let port = state.port;
    let inputs = state.inputs.clone();

    let config = ServerConfig {
        port,
        use_ssl: false,
        cert_file: None,
        key_file: None,
        ca_cert_file: None,
        client_timeout_secs: 30,
    };

    let on_connection = Box::new(move |conn: RtmpConnection| {
        let inputs = inputs.clone();
        let RtmpConnection {
            app,
            stream_key,
            receiver,
        } = conn;
        let app_for_log = app.clone();
        let stream_key_for_log = stream_key.clone();

        let Ok(connection_info) = inputs.update(app, stream_key) else {
            error!("Failed to update RTMP input state");
            return;
        };

        let input_ref = connection_info.input_ref;
        let frame_sender = connection_info.frame_sender;
        let input_samples_sender = connection_info.input_samples_sender;
        let video_decoders = connection_info.video_decoders;
        let ctx = connection_info.ctx;

        thread::spawn(move || {
            let mut decoder_handle = None;
            let mut h264_config: Option<H264AvcDecoderConfig> = None;

            while let Ok(media_data) = receiver.recv() {
                match media_data {
                    RtmpMediaData::VideoConfig(video_config) => {
                        if video_config.codec != FlvVideoCodec::H264 {
                            warn!(?video_config.codec, "Unsupported video codec");
                            continue;
                        }

                        match H264AvcDecoderConfig::parse(video_config.data) {
                            Ok(config) => {
                                h264_config = Some(config);
                                info!("H264 config received")
                            }
                            Err(err) => {
                                warn!(?err, "Failed to parse H264 config")
                            }
                        }
                    }
                    RtmpMediaData::AudioConfig(audio_config) => {
                        info!(?audio_config, "audio config")
                    }
                    RtmpMediaData::Video(video) => {
                        if video.codec != FlvVideoCodec::H264 {
                            warn!(?video.codec, "Unsupported video codec");
                            continue;
                        }

                        if h264_config.is_none() {
                            warn!("Missing H264 config, skipping video until config arrives");
                            continue;
                        }

                        if decoder_handle.is_none() {
                            let transformer = h264_config.clone().map(H264AvccToAnnexB::new);
                            let decoder_thread_options = VideoDecoderThreadOptions {
                                ctx: ctx.clone(),
                                transformer,
                                frame_sender: frame_sender.clone(),
                                input_buffer_size: 10,
                            };

                            let vulkan_supported =
                                ctx.graphics_context.has_vulkan_decoder_support();
                            let h264_decoder = video_decoders.h264.unwrap_or_else(|| {
                                if vulkan_supported {
                                    VideoDecoderOptions::VulkanH264
                                } else {
                                    VideoDecoderOptions::FfmpegH264
                                }
                            });

                            let handle = match h264_decoder {
                                VideoDecoderOptions::FfmpegH264 => {
                                    VideoDecoderThread::<ffmpeg_h264::FfmpegH264Decoder, _>::spawn(
                                        input_ref.clone(),
                                        decoder_thread_options,
                                    )
                                }
                                VideoDecoderOptions::VulkanH264 => {
                                    VideoDecoderThread::<vulkan_h264::VulkanH264Decoder, _>::spawn(
                                        input_ref.clone(),
                                        decoder_thread_options,
                                    )
                                }
                                _ => {
                                    error!("Invalid video decoder provided, expected H264");
                                    continue;
                                }
                            };

                            match handle {
                                Ok(handle) => {
                                    decoder_handle = Some(handle);
                                }
                                Err(err) => {
                                    error!(?err, "Failed to initialize H264 decoder");
                                    continue;
                                }
                            }
                        }

                        let Some(handle) = decoder_handle.as_ref() else {
                            continue;
                        };

                        let pts = Duration::from_millis(video.pts.max(0) as u64);
                        let dts = Duration::from_millis(video.dts.max(0) as u64);

                        let chunk = EncodedInputChunk {
                            data: video.data,
                            pts,
                            dts: Some(dts),
                            kind: MediaKind::Video(VideoCodec::H264),
                        };

                        if handle
                            .chunk_sender
                            .send(PipelineEvent::Data(chunk))
                            .is_err()
                        {
                            warn!("Video decoder channel closed");
                            break;
                        }
                    }
                    RtmpMediaData::Audio(_audio) => {
                        info!("received audio");
                    
                        let _ = input_samples_sender;
                    }
                };
            }

            if let Some(handle) = decoder_handle {
                let _ = handle.chunk_sender.send(PipelineEvent::EOS);
            }
            info!(
                ?app_for_log,
                ?stream_key_for_log,
                "Stream connection closed"
            );
        });
    });

    let mut last_error: Option<RtmpError> = None;
    for _ in 0..5 {
        match RtmpServer::start(config.clone(), on_connection.clone()) {
            Ok(server) => return Ok(server),
            Err(err) => {
                warn!("Failed to start RTMP server. Retrying ...");
                last_error = Some(err)
            }
        }
        thread::sleep(Duration::from_millis(1000));
    }
    Err(InitPipelineError::RtmpServerInitError(last_error.unwrap()))
}
