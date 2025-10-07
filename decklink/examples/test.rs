#[cfg(target_os = "linux")]
mod example {
    use decklink::{
        AudioSampleType, DeckLinkError, DisplayModeType, PixelFormat, SupportedVideoModeFlags,
        VideoConnection, VideoInputConversionMode, VideoInputFlags, get_decklinks,
    };

    pub struct ErrorStack<'a>(Option<&'a (dyn std::error::Error + 'static)>);

    impl<'a> ErrorStack<'a> {
        pub fn new(value: &'a (dyn std::error::Error + 'static)) -> Self {
            ErrorStack(Some(value))
        }

        pub fn into_string(self) -> String {
            let stack: Vec<String> = self.map(ToString::to_string).collect();
            stack.join("\n")
        }
    }

    impl<'a> Iterator for ErrorStack<'a> {
        type Item = &'a (dyn std::error::Error + 'static);

        fn next(&mut self) -> Option<Self::Item> {
            self.0.inspect(|err| {
                self.0 = err.source();
            })
        }
    }

    pub fn example() -> Result<(), DeckLinkError> {
        let decklinks = get_decklinks()?;
        println!("Detected {} decklinks", decklinks.len());
        for deck in &decklinks {
            println!("{:#?}", deck.info()?);
        }
        for deck in &decklinks {
            println!("{:#?}", deck.info()?);
        }

        let decklink = &decklinks[0];

        let input = decklink.input()?;
        let (_is_supported, mode) = input.supports_video_mode(
            VideoConnection::HDMI,
            DisplayModeType::Mode4K2160p60,
            PixelFormat::Format8BitYUV,
            VideoInputConversionMode::NoVideoInputConversion,
            SupportedVideoModeFlags::default(),
        )?;
        println!("{mode:?}");
        input.enable_video(
            mode,
            PixelFormat::Format8BitYUV,
            VideoInputFlags {
                enable_format_detection: true,
                ..Default::default()
            },
        )?;
        input.enable_audio(48_000, AudioSampleType::Sample32bit, 2)?;
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn main() {
    if let Err(err) = example::example() {
        println!("error: {}", example::ErrorStack::new(&err).into_string());
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("Example only available on Linux.");
}
