use bytes::{Bytes, BytesMut};
use hang::catalog::{AudioCodec as MoqAudioCodec, VideoCodec as MoqVideoCodec};
use moq_mux::catalog::{CatalogFormat, Consumer, Stream, hang::Container};
use moq_net::broadcast;
use tracing::{debug, warn};

use super::connection::{AudioTrack, VideoTrack};

use crate::prelude::*;

#[derive(thiserror::Error, Debug)]
pub(crate) enum MoqCatalogError {
    #[error("Failed to subscribe to catalog track")]
    CatalogSubscribeError(#[source] moq_mux::Error),

    #[error("Failed to read catalog")]
    CatalogReadError(#[source] moq_mux::Error),

    #[error("Codec config extraction failed: {0}")]
    CodecConfigExtractionError(&'static str),
}

/// Tracks selected from one catalog snapshot.
pub(super) type CatalogTracks = (Option<VideoTrack>, Option<AudioTrack>);

/// Subscription to the catalog track of a broadcast.
pub(super) struct MoqCatalogReader {
    consumer: Consumer<()>,
    format: CatalogFormat,
}

impl MoqCatalogReader {
    /// Subscribes to the catalog track. Hang catalog is preferred over MSF.
    pub async fn new(broadcast: &broadcast::Consumer) -> Result<Self, MoqCatalogError> {
        let (consumer, format) = match Consumer::<()>::new(broadcast, CatalogFormat::Hang).await {
            Ok(consumer) => (consumer, CatalogFormat::Hang),
            Err(error) => {
                debug!(
                    %error,
                    "Failed to subscribe to Hang catalog, fall back to MSF catalog."
                );
                let consumer = Consumer::<()>::new(broadcast, CatalogFormat::Msf)
                    .await
                    .map_err(MoqCatalogError::CatalogSubscribeError)?;
                (consumer, CatalogFormat::Msf)
            }
        };
        Ok(Self { consumer, format })
    }

    /// Waits for the next catalog snapshot, `None` once the catalog track ends.
    pub async fn next(&mut self) -> Option<Result<CatalogTracks, MoqCatalogError>> {
        let catalog = match self.consumer.next().await.transpose()? {
            Ok(catalog) => catalog,
            Err(err) => return Some(Err(MoqCatalogError::CatalogReadError(err))),
        };
        debug!(format = ?self.format, ?catalog, "Received MoQ catalog");

        let video = find_first_video(&catalog.video);
        let audio = find_first_audio(&catalog.audio);
        Some(Ok((video, audio)))
    }
}

fn find_first_video(video: &hang::catalog::Video) -> Option<VideoTrack> {
    let Some((name, config)) = video.renditions.first_key_value() else {
        return None;
    };
    if config.broadcast.is_some() {
        warn!("Video rendition from another broadcast is not supported; skipping video track.");
        return None;
    }

    let codec = match &config.codec {
        MoqVideoCodec::H264(_) => VideoCodec::H264,
        MoqVideoCodec::VP8 => VideoCodec::Vp8,
        MoqVideoCodec::VP9(_) => VideoCodec::Vp9,
        _ => {
            warn!("Unsupported video codec. Use H264, VP8 or VP9.");
            return None;
        }
    };
    let container = match Container::try_from(config) {
        Ok(container) => container,
        Err(error) => {
            warn!(%error, "Unsupported video container; skipping video track.");
            return None;
        }
    };

    let description = match &config.description {
        Some(desc) => Some(desc.clone()),
        None => match extract_codec_description(&container) {
            Ok(desc) => desc,
            Err(error) => {
                warn!(%error, "Failed to extract video decoder config from container; skipping video track.");
                return None;
            }
        },
    };

    Some(VideoTrack {
        name: name.clone(),
        container,
        codec,
        description,
        config: config.clone(),
    })
}

fn find_first_audio(audio: &hang::catalog::Audio) -> Option<AudioTrack> {
    let Some((name, config)) = audio.renditions.first_key_value() else {
        return None;
    };
    if config.broadcast.is_some() {
        warn!("Audio rendition from another broadcast is not supported; skipping audio track.");
        return None;
    }

    let codec = match &config.codec {
        MoqAudioCodec::Opus => AudioCodec::Opus,
        MoqAudioCodec::AAC(_) => AudioCodec::Aac,
        _ => {
            warn!("Unsupported audio codec. Use AAC or Opus.");
            return None;
        }
    };
    let container = match Container::try_from(config) {
        Ok(container) => container,
        Err(error) => {
            warn!(%error, "Unsupported audio container; skipping audio track.");
            return None;
        }
    };

    let description = match &config.description {
        Some(desc) => Some(desc.clone()),
        None => match extract_codec_description(&container) {
            Ok(desc) => desc,
            Err(error) => {
                warn!(%error, "Failed to extract audio decoder config from container; skipping audio track.");
                return None;
            }
        },
    };

    Some(AudioTrack {
        name: name.clone(),
        container,
        codec,
        description,
        config: config.clone(),
    })
}

fn extract_codec_description(container: &Container) -> Result<Option<Bytes>, MoqCatalogError> {
    use mp4_atom::{Atom, Encode};

    // There is no data to extract from in other containers.
    let Container::Cmaf(cmaf) = container else {
        return Ok(None);
    };

    let codec = cmaf.trak().mdia.minf.stbl.stsd.codecs.first();

    match codec {
        Some(mp4_atom::Codec::Avc1(avc1)) => {
            let mut buf = BytesMut::new();
            avc1.avcc.encode_body(&mut buf).map_err(|_| {
                MoqCatalogError::CodecConfigExtractionError("failed to encode AVCDecoderConfig")
            })?;
            Ok(Some(buf.freeze()))
        }
        Some(mp4_atom::Codec::Mp4a(mp4a)) => {
            let Some(dec_specific) = &mp4a.esds.es_desc.dec_config.dec_specific else {
                return Ok(None);
            };
            let mut buf = BytesMut::new();
            dec_specific.encode(&mut buf).map_err(|_| {
                MoqCatalogError::CodecConfigExtractionError("failed to encode AudioSpecificConfig")
            })?;
            Ok(Some(buf.freeze()))
        }
        _ => Ok(None),
    }
}
