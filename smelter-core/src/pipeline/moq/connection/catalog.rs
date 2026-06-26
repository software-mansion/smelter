use bytes::{Bytes, BytesMut};
use hang::catalog::{
    AudioCodec as MoqAudioCodec, Container as CatalogContainer, VideoCodec as MoqVideoCodec,
};
use moq_mux::catalog::hang::Container;
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
    use moq_mux::catalog::hang::{Catalog, Consumer};

    let catalog_track = broadcast
        .subscribe_track(&hang::Catalog::default_track())
        .map_err(MoqCatalogError::CatalogSubscribeError)?;

    // Each `.next()` call yields the next catalog update. First call yields the initial catalog.
    let catalog = Consumer::new(catalog_track).next().await;
    let catalog: Catalog<()> = catalog
        .map_err(|_| MoqCatalogError::CatalogEmpty)?
        .ok_or(MoqCatalogError::CatalogEmpty)?;

    debug!(?catalog, "Received MoQ Hang catalog");

    let video = discover_video(&catalog.video)?;
    let audio = discover_audio(&catalog.audio)?;

    if video.is_none() && audio.is_none() {
        return Err(MoqCatalogError::CatalogNoTracks);
    }

    Ok(DiscoveredTracks { video, audio })
}

async fn read_msf_catalog(
    broadcast: &BroadcastConsumer,
) -> Result<DiscoveredTracks, MoqCatalogError> {
    use moq_mux::catalog::msf::Consumer;

    let catalog_track = broadcast
        .subscribe_track(&Track::new(moq_msf::DEFAULT_NAME))
        .map_err(MoqCatalogError::CatalogSubscribeError)?;

    // Each `.next()` call yields the next catalog update. First call yields the initial catalog.
    let catalog = Consumer::new(catalog_track).next().await;
    let catalog: hang::Catalog = catalog
        .map_err(|_| MoqCatalogError::CatalogEmpty)?
        .ok_or(MoqCatalogError::CatalogEmpty)?;
    debug!(?catalog, "Received MoQ MSF catalog");

    let video = discover_video(&catalog.video)?;
    let audio = discover_audio(&catalog.audio)?;

    if video.is_none() && audio.is_none() {
        return Err(MoqCatalogError::CatalogNoTracks);
    }

    Ok(DiscoveredTracks { video, audio })
}

fn discover_video(
    video: &hang::catalog::Video,
) -> Result<Option<DiscoveredVideo>, MoqCatalogError> {
    let Some((name, config)) = video.renditions.first_key_value() else {
        return Ok(None);
    };

    let codec = match &config.codec {
        MoqVideoCodec::H264(_) => VideoCodec::H264,
        _ => {
            warn!("Unsupported video codec. Use H264.");
            return Ok(None);
        }
    };
    let container = match &config.container {
        CatalogContainer::Cmaf { init, .. } => Container::Cmaf(fmp4::Wire::from_init(init)?),
        _ => {
            warn!("Unsupported video container. Only CMAF is supported.");
            return Ok(None);
        }
    };

    let description = match (&config.description, &container) {
        (None, Container::Cmaf(wire)) => match extract_codec_description(wire) {
            Ok(desc) => Some(desc),
            Err(error) => {
                warn!(%error, "Failed to extract video decoder config from container; skipping audio track.");
                return Ok(None);
            }
        },
        _ => config.description.clone(),
    };

    Ok(Some(DiscoveredVideo {
        name: name.clone(),
        container,
        codec,
        description,
    }))
}

fn discover_audio(
    audio: &hang::catalog::Audio,
) -> Result<Option<DiscoveredAudio>, MoqCatalogError> {
    let Some((name, config)) = audio.renditions.first_key_value() else {
        return Ok(None);
    };

    let codec = match &config.codec {
        MoqAudioCodec::Opus => AudioCodec::Opus,
        MoqAudioCodec::AAC(_) => AudioCodec::Aac,
        _ => {
            warn!("Unsupported audio codec. Use AAC or Opus.");
            return Ok(None);
        }
    };
    let container = match &config.container {
        CatalogContainer::Cmaf { init, .. } => Container::Cmaf(fmp4::Wire::from_init(init)?),
        _ => {
            warn!("Unsupported audio container. Only CMAF is supported.");
            return Ok(None);
        }
    };

    // Decoder config extraction is necessary only for AAC. Opus is self-contained and does not need
    // description
    let description = match (&config.description, &codec, &container) {
        (None, AudioCodec::Aac, Container::Cmaf(wire)) => match extract_codec_description(wire) {
            Ok(desc) => Some(desc),
            Err(error) => {
                warn!(%error, "Failed to extract audio decoder config from container; skipping audio track.");
                return Ok(None);
            }
        },
        _ => config.description.clone(),
    };

    Ok(Some(DiscoveredAudio {
        name: name.clone(),
        container,
        codec,
        description,
    }))
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
