#[derive(Debug, Clone)]
pub(super) struct WhipInputConnectionState {
    pub bearer_token: Option<String>,
    pub peer_connection: Option<Arc<RTCPeerConnection>>,
    pub start_time_video: Option<Instant>,
    pub start_time_audio: Option<Instant>,
    pub video_decoder_preferences: Vec<VideoDecoder>,
    pub decoded_data_sender: DecodedDataSender,
}

impl WhipInputConnectionState {
    pub fn get_or_initialize_elapsed_start_time(
        &mut self,
        track_kind: RTPCodecType,
    ) -> Option<Duration> {
        match track_kind {
            RTPCodecType::Video => {
                let start_time = self.start_time_video.get_or_insert_with(Instant::now);
                Some(start_time.elapsed())
            }
            RTPCodecType::Audio => {
                let start_time = self.start_time_audio.get_or_insert_with(Instant::now);
                Some(start_time.elapsed())
            }
            _ => None,
        }
    }
}
