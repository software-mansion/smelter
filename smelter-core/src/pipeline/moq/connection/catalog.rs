use bytes::{Bytes, BytesMut};
use hang::catalog::{
    AudioCodec as MoqAudioCodec, Container as CatalogContainer, VideoCodec as MoqVideoCodec,
};
use moq_mux::container::fmp4;
use moq_native::moq_net::{BroadcastConsumer, Error as MoqError, Track};
use tracing::{debug, warn};

use crate::pipeline::moq::connection::{DiscoveredAudio, DiscoveredTracks, DiscoveredVideo};

use crate::prelude::*;

#[derive(thiserror::Error, Debug)]
pub(super) enum MoqCatalogError {
    #[error("Failed to subscribe to catalog track")]
    CatalogSubscribeError(#[source] MoqError),

    #[error("Catalog track produced no frames")]
    CatalogEmpty,

    #[error("Catalog contains no recognizable video or audio tracks")]
    CatalogNoTracks,

    #[error("CMAF parse error: {0}")]
    CmafParseError(#[from] fmp4::Error),

    #[error("Codec config extraction failed: {0}")]
    CodecConfigExtractionError(&'static str),
}

pub(super) async fn read_catalog(
    broadcast: &BroadcastConsumer,
) -> Result<DiscoveredTracks, MoqCatalogError> {
    match read_hang_catalog(broadcast).await {
        Ok(tracks) => Ok(tracks),
        Err(error) => {
            debug!(
                %error,
                "Failed to read Hang catalog, fall back to MSF catalog."
            );
            read_msf_catalog(broadcast).await
        }
    }
}

async fn read_hang_catalog(
    broadcast: &BroadcastConsumer,
) -> Result<DiscoveredTracks, MoqCatalogError> {
    use moq_mux::catalog::hang::{Catalog, Consumer, Container};

    let catalog_track = broadcast
        .subscribe_track(&hang::Catalog::default_track())
        .map_err(MoqCatalogError::CatalogSubscribeError)?;

    // Each `.next()` call yields the next catalog update. First call yields the initial catalog.
    let catalog = Consumer::new(catalog_track).next().await;
    let catalog: Catalog<()> = catalog
        .map_err(|_| MoqCatalogError::CatalogEmpty)?
        .ok_or(MoqCatalogError::CatalogEmpty)?;

    debug!(?catalog, "Received MoQ Hang catalog");

    let video = match catalog.video.renditions.first_key_value() {
        Some((name, config)) => match (video_codec(&config.codec), &config.container) {
            (Ok(codec), CatalogContainer::Cmaf { init, .. }) => {
                let wire = fmp4::Wire::from_init(init)?;
                Some(DiscoveredVideo {
                    name: name.clone(),
                    container: Container::Cmaf(wire),
                    codec,
                    description: config.description.clone(),
                })
            }
            (codec_res, container) => {
                if let Err(msg) = codec_res {
                    warn!("{msg}");
                }
                if !matches!(container, CatalogContainer::Cmaf { .. }) {
                    warn!("Unsupported video container, only CMAF is supported.");
                }
                None
            }
        },
        None => None,
    };

    let audio = match catalog.audio.renditions.first_key_value() {
        Some((name, config)) => match (audio_codec(&config.codec), &config.container) {
            (Ok(codec), CatalogContainer::Cmaf { init, .. }) => {
                let wire = fmp4::Wire::from_init(init)?;
                Some(DiscoveredAudio {
                    name: name.clone(),
                    container: Container::Cmaf(wire),
                    codec,
                    description: config.description.clone(),
                })
            }
            (codec_res, container) => {
                if let Err(msg) = codec_res {
                    warn!("{msg}");
                }
                if !matches!(container, CatalogContainer::Cmaf { .. }) {
                    warn!("Unsupported audio container, only CMAF is supported.");
                }
                None
            }
        },
        None => None,
    };

    if video.is_none() && audio.is_none() {
        return Err(MoqCatalogError::CatalogNoTracks);
    }

    Ok(DiscoveredTracks { video, audio })
}

async fn read_msf_catalog(
    broadcast: &BroadcastConsumer,
) -> Result<DiscoveredTracks, MoqCatalogError> {
    use moq_mux::catalog::{hang::Container, msf::Consumer};

    let catalog_track = broadcast
        .subscribe_track(&Track::new(moq_msf::DEFAULT_NAME))
        .map_err(MoqCatalogError::CatalogSubscribeError)?;

    // Each `.next()` call yields the next catalog update. First call yields the initial catalog.
    let catalog = Consumer::new(catalog_track).next().await;
    let catalog: hang::Catalog = catalog
        .map_err(|_| MoqCatalogError::CatalogEmpty)?
        .ok_or(MoqCatalogError::CatalogEmpty)?;
    debug!(?catalog, "Received MoQ MSF catalog");

    let video = match catalog.video.renditions.first_key_value() {
        Some((name, config)) => match (video_codec(&config.codec), &config.container) {
            (Ok(codec), CatalogContainer::Cmaf { init, .. }) => {
                let wire = fmp4::Wire::from_init(init)?;
                let description = match extract_codec_description(&wire) {
                    Ok(config) => Some(config),
                    Err(error) => {
                        warn!(%error, "Failed to extract video config from CMAF container.");
                        None
                    }
                };

                Some(DiscoveredVideo {
                    name: name.clone(),
                    container: Container::Cmaf(wire),
                    codec,
                    description,
                })
            }
            (codec_res, container) => {
                if let Err(msg) = codec_res {
                    warn!("{msg}");
                }
                if !matches!(container, CatalogContainer::Cmaf { .. }) {
                    warn!("Unsupported video container, only CMAF is supported.");
                }
                None
            }
        },
        None => None,
    };

    let audio = match catalog.audio.renditions.first_key_value() {
        Some((name, config)) => match (audio_codec(&config.codec), &config.container) {
            // TODO: (@jbrs): It needs to be reconsidered how decoder config should be handled,
            // where should it be extracted from the container, here or in the decoder.
            // Return to that when adding additional containers.
            (Ok(codec), CatalogContainer::Cmaf { init, .. }) => {
                let wire = fmp4::Wire::from_init(init)?;
                let description = match &config.codec {
                    MoqAudioCodec::AAC(_) => match extract_codec_description(&wire) {
                        Ok(config) => Some(config),
                        Err(error) => {
                            warn!(%error, "Failed to extract AAC audio config from CMAF container.");
                            None
                        }
                    },
                    _ => None,
                };

                Some(DiscoveredAudio {
                    name: name.clone(),
                    container: Container::Cmaf(wire),
                    codec,
                    description,
                })
            }
            (codec_res, container) => {
                if let Err(msg) = codec_res {
                    warn!("{msg}");
                }
                if !matches!(container, CatalogContainer::Cmaf { .. }) {
                    warn!("Unsupported audio container, only CMAF is supported.");
                }
                None
            }
        },
        None => None,
    };

    if video.is_none() && audio.is_none() {
        return Err(MoqCatalogError::CatalogNoTracks);
    }

    Ok(DiscoveredTracks { video, audio })
}

fn extract_codec_description(cmaf: &fmp4::Wire) -> Result<Bytes, MoqCatalogError> {
    use mp4_atom::{Atom, Encode};

    let codec = cmaf.trak().mdia.minf.stbl.stsd.codecs.first().ok_or(
        MoqCatalogError::CodecConfigExtractionError("CMAF init segment contains no codec entries"),
    )?;

    match codec {
        mp4_atom::Codec::Avc1(avc1) => {
            let mut buf = BytesMut::new();
            avc1.avcc.encode_body(&mut buf).map_err(|_| {
                MoqCatalogError::CodecConfigExtractionError("failed to encode AVCDecoderConfig")
            })?;
            Ok(buf.freeze())
        }
        mp4_atom::Codec::Mp4a(mp4a) => {
            let mut buf = BytesMut::new();
            mp4a.esds
                .es_desc
                .dec_config
                .dec_specific
                .encode(&mut buf)
                .map_err(|_| {
                    MoqCatalogError::CodecConfigExtractionError(
                        "failed to encode AudioSpecificConfig",
                    )
                })?;
            Ok(buf.freeze())
        }
        _ => Err(MoqCatalogError::CodecConfigExtractionError(
            "unsupported codec in CMAF init segment",
        )),
    }
}

fn audio_codec(codec: &MoqAudioCodec) -> Result<AudioCodec, &'static str> {
    match codec {
        MoqAudioCodec::Opus => Ok(AudioCodec::Opus),
        MoqAudioCodec::AAC(_) => Ok(AudioCodec::Aac),
        _ => Err("Unsupported audio codec. Use AAC or Opus."),
    }
}

fn video_codec(codec: &MoqVideoCodec) -> Result<VideoCodec, &'static str> {
    match codec {
        MoqVideoCodec::H264(_) => Ok(VideoCodec::H264),
        _ => Err("Unsupported video codec. Use H264."),
    }
}
