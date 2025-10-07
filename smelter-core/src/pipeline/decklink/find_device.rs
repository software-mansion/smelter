use decklink::{
    FlagAttributeId, IntegerAttributeId, StringAttributeId, VideoIOSupport, get_decklinks,
};

use super::{DeckLinkDeviceInfo, DeckLinkInputError, DeckLinkInputOptions};

pub(super) fn find_decklink(
    opts: &DeckLinkInputOptions,
) -> Result<decklink::DeckLink, DeckLinkInputError> {
    let decklinks = get_decklinks()?;

    let decklinks_info = decklinks
        .iter()
        .map(|decklink| {
            let attr = decklink.profile_attributes()?;
            Ok(DeckLinkDeviceInfo {
                display_name: attr.get_string(StringAttributeId::DisplayName)?,
                persistent_id: attr
                    .get_integer(IntegerAttributeId::PersistentID)?
                    .map(|value| format!("{value:X}")),
                subdevice_index: attr
                    .get_integer(IntegerAttributeId::SubDeviceIndex)?
                    .map(|i| i as u32),
            })
        })
        .collect::<Result<_, DeckLinkInputError>>()?;

    for mut decklink in decklinks.into_iter() {
        if is_selected_decklink(opts, &mut decklink)? {
            return Ok(decklink);
        }
    }

    Err(DeckLinkInputError::NoMatchingDeckLink(decklinks_info))
}

fn is_selected_decklink(
    opts: &DeckLinkInputOptions,
    decklink: &mut decklink::DeckLink,
) -> Result<bool, DeckLinkInputError> {
    let attr = decklink.profile_attributes()?;

    if let Some(subdevice) = opts.subdevice_index
        && attr.get_integer(IntegerAttributeId::SubDeviceIndex)? != Some(subdevice.into())
    {
        return Ok(false);
    }

    if let Some(display_name) = &opts.display_name
        && attr.get_string(StringAttributeId::DisplayName)?.as_ref() != Some(display_name)
    {
        return Ok(false);
    }

    if let Some(persistent_id) = opts.persistent_id
        && attr.get_integer(IntegerAttributeId::PersistentID)? != Some(persistent_id as i64)
    {
        return Ok(false);
    }

    let video_io_support = VideoIOSupport::from(
        attr.get_integer(IntegerAttributeId::VideoIOSupport)?
            .ok_or(DeckLinkInputError::NoCaptureSupport)?,
    );
    if !video_io_support.capture {
        return Err(DeckLinkInputError::NoCaptureSupport);
    }

    if attr.get_flag(FlagAttributeId::SupportsInputFormatDetection)? != Some(true) {
        return Err(DeckLinkInputError::NoInputFormatDetection);
    }

    Ok(true)
}
