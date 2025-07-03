use crate::audio_mixer::OutputSamples;

mod instance;

const SAMPLE_BATCH_DURATION: Duration = Duration::from_millis(20);

enum SamplesType {
    Mono,
    Stereo,
}

impl SamplesType {
    fn new(output_samples: &OutputSamples) -> Self {
        match &output_samples.samples {
            AudioSamples::Mono(_) => Self::Mono,
            AudioSamples::Stereo(_) => Self::Stereo,
        }
    }
}


