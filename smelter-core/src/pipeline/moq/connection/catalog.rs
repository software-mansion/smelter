use bytes::Bytes;
use hang::catalog::{AudioCodec, Container, VideoCodec};
use moq_mux::{
    catalog::{hang::Consumer as HangConsumer, msf::Consumer as MsfConsumer},
    container::fmp4,
};
use moq_native::moq_net::{self, BroadcastConsumer};

use crate::pipeline::moq::connection::{
    DiscoveredAudio, DiscoveredTracks, DiscoveredVideo, MoqCatalogError,
};
use tracing::{debug, warn};

pub(super) async fn read_catalog(
    broadcast: &BroadcastConsumer,
) -> Result<DiscoveredTracks, MoqCatalogError> {
    // Handle moq-lite "catalog.json", if it is not present fallback to the standard msf "catalog"
    match read_hang_catalog(broadcast).await {
        Ok(discovered_tracks) => Ok(discovered_tracks),
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
        Some((name, config)) if let VideoCodec::H264(_) = config.codec => match &config.container {
            Container::Cmaf { init, .. } => {
                let container = fmp4::Wire::from_init(init)?;

                Some(DiscoveredVideo {
                    name: name.clone(),
                    container,
                    description: config.description.clone(),
                })
            }
            _ => {
                warn!("Only CMAF container is supported.");
                None
            }
        },
        Some((name, config)) => {
            warn!(track=%name, codec=%config.codec, "Unsupported video codec, skipping track");
            None
        }
        None => None,
    };

    let audio = match catalog.audio.renditions.first_key_value() {
        Some((name, config)) if let AudioCodec::AAC(_) = config.codec => match &config.container {
            Container::Cmaf { init, .. } => {
                let container = fmp4::Wire::from_init(init)?;

                Some(DiscoveredAudio {
                    name: name.clone(),
                    container,
                    description: config.description.clone(),
                })
            }
            _ => {
                warn!("Only CMAF container is supported.");
                None
            }
        },
        Some((name, config)) => {
            warn!(track=%name, codec=%config.codec, "Unsupported audio codec, skipping track");
            None
        }
        None => None,
    };

    if video.is_none() && audio.is_none() {
        return Err(MoqCatalogError::CatalogNoTracks);
    }

    Ok(DiscoveredTracks { video, audio })
}

async fn read_msf_catalog(
    broadcast: &BroadcastConsumer,
) -> Result<DiscoveredTracks, MoqConnectionError> {
    let mut catalog_track = broadcast
        .subscribe_track(&moq_net::Track::new(moq_msf::DEFAULT_NAME))
        .map_err(MoqConnectionError::CatalogSubscribeError)?;

    let frame = catalog_track
        .read_frame()
        .await?
        .ok_or(MoqConnectionError::CatalogEmpty)?;

    let catalog = moq_msf::Catalog::from_str(
        std::str::from_utf8(&frame).map_err(|_| MoqConnectionError::CatalogParseError)?,
    )
    .map_err(|_| MoqConnectionError::CatalogParseError)?;
    debug!(?catalog, "Received MoQ MSF catalog");

    let video = catalog
        .tracks
        .iter()
        .find(|t| {
            t.role == Some(moq_msf::Role::Video)
                && t.codec
                    .as_deref()
                    .is_some_and(|c| c.starts_with("avc1") || c.starts_with("avc3"))
        })
        .and_then(|t| match msf_track_to_hang(t) {
            Ok((container, description)) => Some(DiscoveredVideo {
                name: t.name.clone(),
                container,
                description,
            }),
            Err(error) => {
                warn!(track=%t.name, error, "Skipping MSF video track.");
                None
            }
        });

    let audio = catalog
        .tracks
        .iter()
        .find(|t| {
            t.role == Some(moq_msf::Role::Audio)
                && t.codec.as_deref().is_some_and(|c| c.starts_with("mp4a"))
        })
        .and_then(|t| match msf_track_to_hang(t) {
            Ok((container, description)) => Some(DiscoveredAudio {
                name: t.name.clone(),
                container,
                description,
            }),
            Err(error) => {
                warn!(track=%t.name, error, "Skipping MSF audio track.");
                None
            }
        });

    if video.is_none() && audio.is_none() {
        return Err(MoqConnectionError::CatalogNoTracks);
    }

    Ok(DiscoveredTracks { video, audio })
}

fn msf_track_to_hang(track: &moq_msf::Track) -> Result<(Hang, Option<Bytes>), &'static str> {
    let init_data = track
        .init_data
        .as_deref()
        .map(|b64| {
            data_encoding::BASE64
                .decode(b64.as_bytes())
                .map(Bytes::from)
        })
        .transpose()
        .map_err(|_| "invalid base64 init_data")?;

    let container = match track.packaging {
        moq_msf::Packaging::Cmaf => {
            let init_bytes = init_data
                .as_ref()
                .ok_or("CMAF packaging requires init_data")?;
            Hang::Cmaf(
                Cmaf::from_init(init_bytes).map_err(|_| "failed to parse CMAF init segment")?,
            )
        }
        _ => return Err("Unsupported packaging mode, only CMAF mode is supported."),
    };

    let description = match &container {
        Hang::Cmaf(cmaf) => Some(extract_codec_description(cmaf)?),
        _ => None,
    };

    Ok((container, description))
}

fn extract_codec_description(cmaf: &Cmaf) -> Result<Bytes, &'static str> {
    use mp4_atom::{Atom, Encode};

    let codec = cmaf
        .trak()
        .mdia
        .minf
        .stbl
        .stsd
        .codecs
        .first()
        .ok_or("CMAF init segment contains no codec entries")?;

    let mut buf = Vec::new();
    match codec {
        mp4_atom::Codec::Avc1(avc1) => {
            avc1.avcc
                .encode_body(&mut buf)
                .map_err(|_| "failed to encode AVCDecoderConfig")?;
        }
        mp4_atom::Codec::Mp4a(mp4a) => {
            mp4a.esds
                .es_desc
                .dec_config
                .dec_specific
                .encode(&mut buf)
                .map_err(|_| "failed to encode AudioSpecificConfig")?;
        }
        _ => return Err("unsupported codec in CMAF init segment"),
    }

    Ok(Bytes::from(buf))
}
