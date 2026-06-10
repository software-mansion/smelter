use bytes::{Bytes, BytesMut};
use hang::catalog::{AudioCodec, Container as CatalogContainer, VideoCodec};
use moq_mux::{
    catalog::{hang::Consumer as HangConsumer, hang::Container, msf::Consumer as MsfConsumer},
    container::fmp4,
};
use moq_native::moq_net::{self, BroadcastConsumer, Error as MoqError};

use crate::pipeline::moq::connection::{DiscoveredAudio, DiscoveredTracks, DiscoveredVideo};
use tracing::{debug, warn};

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
        Err(_) => read_msf_catalog(broadcast).await,
    }
}

async fn read_hang_catalog(
    broadcast: &BroadcastConsumer,
) -> Result<DiscoveredTracks, MoqCatalogError> {
    let catalog_track = broadcast
        .subscribe_track(&hang::Catalog::default_track())
        .map_err(MoqCatalogError::CatalogSubscribeError)?;

    let mut consumer = HangConsumer::new(catalog_track);

    let catalog = consumer
        .next()
        .await
        .map_err(|_| MoqCatalogError::CatalogEmpty)?
        .ok_or(MoqCatalogError::CatalogEmpty)?;
    debug!(?catalog, "Received MoQ Hang catalog");

    let video = match catalog.video.renditions.first_key_value() {
        Some((name, config)) => match (&config.container, &config.codec) {
            (CatalogContainer::Cmaf { init, .. }, VideoCodec::H264(_)) => {
                let wire = fmp4::Wire::from_init(init)?;
                let container = Container::Cmaf(wire);

                Some(DiscoveredVideo {
                    name: name.clone(),
                    container,
                    description: config.description.clone(),
                })
            }
            _ => {
                warn!("Only CMAF container with H264 encoded video is supported.");
                None
            }
        },
        None => None,
    };

    let audio = match catalog.audio.renditions.first_key_value() {
        Some((name, config)) => match (&config.container, &config.codec) {
            (CatalogContainer::Cmaf { init, .. }, AudioCodec::AAC(_)) => {
                let wire = fmp4::Wire::from_init(init)?;
                let container = Container::Cmaf(wire);

                Some(DiscoveredAudio {
                    name: name.clone(),
                    container,
                    description: config.description.clone(),
                })
            }
            _ => {
                warn!("Only CMAF container with AAC encoded audio is supported.");
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
    let catalog_track = broadcast
        .subscribe_track(&moq_net::Track::new(moq_msf::DEFAULT_NAME))
        .map_err(MoqCatalogError::CatalogSubscribeError)?;

    let mut consumer = MsfConsumer::new(catalog_track);

    let catalog = consumer
        .next()
        .await
        .map_err(|_| MoqCatalogError::CatalogEmpty)?
        .ok_or(MoqCatalogError::CatalogEmpty)?;
    debug!(?catalog, "Received MoQ MSF catalog");

    let video = match catalog.video.renditions.first_key_value() {
        Some((name, config)) => match (&config.container, &config.codec) {
            (CatalogContainer::Cmaf { init, .. }, VideoCodec::H264(_)) => {
                let wire = fmp4::Wire::from_init(init)?;
                let description = match extract_codec_description(&wire) {
                    Ok(config) => Some(config),
                    Err(error) => {
                        warn!(%error, "Failed to extract video config from CMAF container.");
                        None
                    }
                };
                let container = Container::Cmaf(wire);

                Some(DiscoveredVideo {
                    name: name.clone(),
                    container,
                    description,
                })
            }
            _ => {
                warn!("Only CMAF container with H264 encoded video is supported.");
                None
            }
        },
        None => None,
    };

    let audio = match catalog.audio.renditions.first_key_value() {
        Some((name, config)) => match (&config.container, &config.codec) {
            (CatalogContainer::Cmaf { init, .. }, AudioCodec::AAC(_)) => {
                let wire = fmp4::Wire::from_init(init)?;
                let description = match extract_codec_description(&wire) {
                    Ok(config) => Some(config),
                    Err(error) => {
                        warn!(%error, "Failed to extract audio config from CMAF container.");
                        None
                    }
                };
                let container = Container::Cmaf(wire);

                Some(DiscoveredAudio {
                    name: name.clone(),
                    container,
                    description,
                })
            }
            _ => {
                warn!("Only CMAF container with AAC encoded audio is supported.");
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
