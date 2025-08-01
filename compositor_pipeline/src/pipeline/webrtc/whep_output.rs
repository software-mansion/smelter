use std::{sync::Arc, time::Duration};

use crossbeam_channel::bounded;
use rand::Rng;
use tokio::sync::{mpsc, oneshot, watch};

use webrtc::track::track_local::{track_local_static_rtp::TrackLocalStaticRTP, TrackLocalWriter};

use crate::pipeline::{
    encoder::{
        ffmpeg_h264::FfmpegH264Encoder, ffmpeg_vp8::FfmpegVp8Encoder, ffmpeg_vp9::FfmpegVp9Encoder,
        VideoEncoderConfig,
    },
    output::{Output, OutputAudio, OutputVideo},
    rtp::{
        payloader::{PayloadedCodec, PayloaderOptions},
        RtpPacket,
    },
    webrtc::{
        bearer_token::generate_token,
        whep_output::{
            connection_state::WhepOutputConnectionStateOptions,
            track_task_audio::WhepAudioTrackThreadHandle,
            track_task_video::{spawn_video_track_thread, WhepVideoTrackThreadHandle},
        },
    },
};

use crate::prelude::*;

pub(super) mod connection_state;
pub(super) mod state;

pub(super) mod peer_connection;
pub(super) mod setup_track;
pub(super) mod track_task_audio;
pub(super) mod track_task_video;

pub(super) use state::WhepOutputsState;

const WHEP_INIT_TIMEOUT: Duration = Duration::from_secs(60);

pub struct WhepSenderTrack {
    pub receiver: mpsc::Receiver<RtpPacket>,
    pub track: Arc<TrackLocalStaticRTP>,
}

// struct WhepTask {
//     // ctx: Arc<PipelineCtx>,
//     // output_id: OutputId,
//     video_track: Option<WhepSenderTrack>,
//     audio_track: Option<WhepSenderTrack>,
// }

// impl WhepTask {
//     async fn new(
//         ctx: Arc<PipelineCtx>,
//         output_id: OutputId,
//         whep_outputs_state: WhepOutputsState,
//         options: WhepSenderOptions,
//     ) -> Result<(Self, WhepOutput), WhepOutputError> {
//         let pc = PeerConnection::new(&ctx, &options).await?;

//         let (video_rtc_sender, video_track) = pc.new_video_track().await?;
//         let (audio_rtc_sender, audio_track) = pc.new_audio_track().await?;

//         let (video_thread_handle, video_track) = match &options.video {
//             Some(opts) => {
//                 let (video_thread_handle, video) =
//                     setup_video_track(&ctx, &output_id, video_rtc_sender, video_track, opts)
//                         .await?;
//                 (Some(video_thread_handle), Some(video))
//             }
//             None => (None, None),
//         };

//         let (audio_thread_handle, audio_track) = match &options.audio {
//             Some(opts) => {
//                 let (audio_thread_handle, audio) = setup_audio_track(
//                     &ctx,
//                     &output_id,
//                     audio_rtc_sender,
//                     audio_track,
//                     pc.clone(),
//                     opts,
//                 )
//                 .await?;
//                 (Some(audio_thread_handle), Some(audio))
//             }
//             None => (None, None),
//         };

//         Ok((
//             Self {
//                 // ctx: ctx.clone(),
//                 // output_id: output_id.clone(),
//                 video_track,
//                 audio_track,
//             },
//             WhepOutput {
//                 whep_outputs_state,
//                 output_id,
//                 video: video_thread_handle,
//                 audio: audio_thread_handle,
//             },
//         ))
//     }

//     async fn run(self) {
//         let (mut audio_receiver, audio_track) = match self.audio_track {
//             Some(WhepSenderTrack { receiver, track }) => (Some(receiver), Some(track)),
//             None => (None, None),
//         };

//         let (mut video_receiver, video_track) = match self.video_track {
//             Some(WhepSenderTrack { receiver, track }) => (Some(receiver), Some(track)),
//             None => (None, None),
//         };
//         let mut next_video_packet = None;
//         let mut next_audio_packet = None;

//         loop {
//             match (
//                 &next_video_packet,
//                 &next_audio_packet,
//                 &mut video_receiver,
//                 &mut audio_receiver,
//             ) {
//                 (None, None, Some(video_receiver), Some(audio_receiver)) => {
//                     tokio::select! {
//                         Some(packet) = video_receiver.recv() => {
//                             next_video_packet = Some(packet)
//                         },
//                         Some(packet) = audio_receiver.recv() => {
//                             next_audio_packet = Some(packet)
//                         },
//                         else => break,
//                     };
//                 }
//                 (_video, None, _video_receiver, audio_receiver @ Some(_)) => {
//                     match audio_receiver.as_mut().unwrap().recv().await {
//                         Some(packet) => {
//                             next_audio_packet = Some(packet);
//                         }
//                         None => *audio_receiver = None,
//                     };
//                 }
//                 (None, _, video_receiver @ Some(_), _) => {
//                     match video_receiver.as_mut().unwrap().recv().await {
//                         Some(packet) => {
//                             next_video_packet = Some(packet);
//                         }
//                         None => *video_receiver = None,
//                     };
//                 }
//                 (None, None, None, None) => {
//                     break;
//                 }
//                 (Some(_), Some(_), _, _) => {
//                     warn!("Both packets populated, this should not happen.");
//                 }
//                 (None, Some(_audio), None, _) => {
//                     // no video, but can't read audio at this moment
//                 }
//                 (Some(_video), None, _, None) => {
//                     // no audio, but can't read video at this moment
//                 }
//             };

//             match (&next_video_packet, &next_audio_packet) {
//                 // try to wait for both audio and video packet to be ready
//                 (Some(video), Some(audio)) => {
//                     if audio.timestamp > video.timestamp {
//                         if let (Some(packet), Some(track)) =
//                             (next_video_packet.take(), &video_track)
//                         {
//                             if let Err(err) = track.write_rtp(&packet.packet).await {
//                                 warn!("RTP write error {}", err);
//                                 break;
//                             }
//                         }
//                     } else if let (Some(packet), Some(track)) =
//                         (next_audio_packet.take(), &audio_track)
//                     {
//                         if let Err(err) = track.write_rtp(&packet.packet).await {
//                             warn!("RTP write error {}", err);
//                             break;
//                         }
//                     }
//                 }
//                 // read audio if there is not way to get video packet
//                 (None, Some(_)) if video_receiver.is_none() => {
//                     if let (Some(p), Some(track)) = (next_audio_packet.take(), &audio_track) {
//                         if let Err(err) = track.write_rtp(&p.packet).await {
//                             warn!("RTP write error {}", err);
//                             break;
//                         }
//                     }
//                 }
//                 // read video if there is not way to get audio packet
//                 (Some(_), None) if audio_receiver.is_none() => {
//                     if let (Some(p), Some(track)) = (next_video_packet.take(), &video_track) {
//                         if let Err(err) = track.write_rtp(&p.packet).await {
//                             warn!("RTP write error {}", err);
//                             break;
//                         }
//                     }
//                 }
//                 (None, None) => break,
//                 // we can't do anything here, but there are still receivers
//                 // that can return something in the next loop.
//                 //
//                 // I don't think this can ever happen
//                 (_, _) => (),
//             };
//         }

//         // self.ctx
//         //     .event_emitter
//         //     .emit(Event::OutputDone(self.output_id));
//         debug!("Closing WHEP sender thread.")
//     }
// }

impl Output for WhepOutput {
    fn audio(&self) -> Option<OutputAudio> {
        self.audio.as_ref().map(|audio| OutputAudio {
            samples_batch_sender: &audio.sample_batch_sender,
        })
    }

    fn video(&self) -> Option<OutputVideo> {
        self.video.as_ref().map(|video| OutputVideo {
            resolution: video.config.resolution,
            frame_format: video.config.output_format,
            frame_sender: &video.frame_sender,
            keyframe_request_sender: &video.keyframe_request_sender,
        })
    }

    fn kind(&self) -> OutputProtocolKind {
        OutputProtocolKind::Whep
    }
}

// fn wait_with_deadline<T>(
//     mut result_receiver: oneshot::Receiver<Result<T, WhepOutputError>>,
//     timeout: Duration,
// ) -> Result<T, OutputInitError> {
//     let start_time = Instant::now();
//     while start_time.elapsed() < timeout {
//         thread::sleep(Duration::from_millis(500));

//         match result_receiver.try_recv() {
//             Ok(result) => match result {
//                 Ok(handle) => return Ok(handle),
//                 Err(err) => return Err(OutputInitError::WhepInitError(err.into())),
//             },
//             Err(err) => match err {
//                 oneshot::error::TryRecvError::Closed => {
//                     return Err(OutputInitError::UnknownWhepError)
//                 }
//                 oneshot::error::TryRecvError::Empty => {}
//             },
//         };
//     }
//     result_receiver.close();
//     Err(OutputInitError::WhepInitTimeout)
// }

#[derive(Debug)]
pub struct WhepOutput {
    whep_outputs_state: WhepOutputsState,
    output_id: OutputId,
    video: Option<WhepVideoTrackThreadHandle>,
    audio: Option<WhepAudioTrackThreadHandle>,
}

impl WhepOutput {
    pub(crate) fn new(
        ctx: Arc<PipelineCtx>,
        output_id: OutputId,
        options: WhepSenderOptions,
    ) -> Result<Self, OutputInitError> {
        let state_clone = ctx.whip_whep_state.clone();
        let Some(state) = state_clone else {
            return Err(OutputInitError::WhipWhepServerNotRunning);
        };

        let (frame_sender, frame_receiver) = bounded(5);
        let (sample_batch_sender, samples_batch_receiver) = bounded(5);

        let bearer_token = options.bearer_token.clone().unwrap_or_else(generate_token);
        state.outputs.add_output(
            &output_id,
            WhepOutputConnectionStateOptions {
                bearer_token: bearer_token.clone(),
                video_encoder: options.video.clone().map(|v| v.encoder),
                audio_encoder: options.audio.clone().map(|a| a.encoder),
                frame_receiver,
                output_samples_receiver: samples_batch_receiver,
            },
        );

        fn payloader_options(
            codec: PayloadedCodec,
            payload_type: u8,
            ssrc: u32,
        ) -> PayloaderOptions {
            PayloaderOptions {
                codec,
                payload_type,
                clock_rate: 90_000,
                mtu: 1200,
                ssrc,
            }
        }

        // if let Some(video_options) = options.video {
        //     let ssrc = rand::thread_rng().gen::<u32>();
        //     let handle = match video_options.encoder {
        //         VideoEncoderOptions::FfmpegH264(options) => {
        //             spawn_video_track_thread::<FfmpegH264Encoder>(
        //                 ctx.clone(),
        //                 output_id.clone(),
        //                 options,
        //                 payloader_options(PayloadedCodec::H264, 102, ssrc),
        //                 sender,
        //             )
        //         }
        //         VideoEncoderOptions::FfmpegVp8(options) => {
        //             spawn_video_track_thread::<FfmpegVp8Encoder>(
        //                 ctx.clone(),
        //                 output_id.clone(),
        //                 options,
        //                 payloader_options(PayloadedCodec::Vp8, 96, ssrc),
        //                 sender,
        //             )
        //         }
        //         VideoEncoderOptions::FfmpegVp9(options) => {
        //             spawn_video_track_thread::<FfmpegVp9Encoder>(
        //                 ctx.clone(),
        //                 output_id.clone(),
        //                 options,
        //                 payloader_options(PayloadedCodec::Vp9, 98, ssrc),
        //                 sender,
        //             )
        //         }
        //     }
        //     .unwrap();
        // }

        let (keyframe_sender, keyframe_receiver) = crossbeam_channel::bounded(1);
        let (packet_loss_sender, packet_loss_receiver) = watch::channel(1);

        println!("{:?}", options.video);

        Ok(WhepOutput {
            video: Some(WhepVideoTrackThreadHandle {
                frame_sender,
                keyframe_request_sender: keyframe_sender,
                config: VideoEncoderConfig {
                    resolution: Resolution {
                        width: 1920,
                        height: 1080,
                    },
                    output_format: compositor_render::OutputFrameFormat::PlanarYuv420Bytes,
                    extradata: None,
                },
            }),
            audio: Some(WhepAudioTrackThreadHandle {
                sample_batch_sender,
                packet_loss_sender,
            }),
            whep_outputs_state: state.outputs.clone(),
            output_id,
        })
    }
}

impl Drop for WhepOutput {
    fn drop(&mut self) {
        self.whep_outputs_state
            .ensure_output_closed(&self.output_id);
    }
}
