use hang::{
    Catalog,
    catalog::{AudioCodec, Container as CatalogContainer, VideoCodec},
};
use moq_mux::{
    catalog::{hang::Consumer as HangConsumer, hang::Container, msf::Consumer as MsfConsumer},
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
    let catalog = match read_hang_catalog(broadcast).await {
        Ok(catalog) => catalog,
        Err(_) => read_msf_catalog(broadcast).await?,
    };

    let video = match catalog.video.renditions.first_key_value() {
        Some((name, config)) if let VideoCodec::H264(_) = config.codec => match &config.container {
            CatalogContainer::Cmaf { init, .. } => {
                let wire = fmp4::Wire::from_init(init)?;
                let container = Container::Cmaf(wire);

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
            CatalogContainer::Cmaf { init, .. } => {
                let wire = fmp4::Wire::from_init(init)?;
                let container = Container::Cmaf(wire);

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

async fn read_hang_catalog(broadcast: &BroadcastConsumer) -> Result<Catalog, MoqCatalogError> {
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

    Ok(catalog)
}

async fn read_msf_catalog(broadcast: &BroadcastConsumer) -> Result<Catalog, MoqCatalogError> {
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

    Ok(catalog)
}
