//! Tests of output scenes defined by a Lua script. Smelter runs in offline mode with raw frames on
//! input and output, so every output frame can be checked.

use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use bytes::Bytes;
use crossbeam_channel::Receiver;
use smelter::{config::read_config, state::pipeline_options_from_config};
use smelter_core::{
    error::{RegisterOutputError, UpdateOutputError},
    graphics_context::GraphicsContext,
    protocols::*,
    *,
};
use smelter_render::{
    FrameData, Framerate, InputId, OutputId, RendererId, RendererSpec, Resolution, YuvPlanes,
    image::{ImageSource, ImageSpec, ImageType},
    scene::{Component, RGBAColor, Scene, ViewComponent},
};

use crate::{paths::integration_tests_root, read_rgba_texture};

const RESOLUTION: Resolution = Resolution {
    width: 640,
    height: 360,
};

const FRAMERATE: Framerate = Framerate { num: 30, den: 1 };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Green,
    Blue,
    Other,
}

struct TestPipeline {
    pipeline: Arc<Mutex<Pipeline>>,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl TestPipeline {
    fn new() -> Self {
        let mut config = read_config();
        config.ahead_of_time_processing = true;
        config.never_drop_output_frames = true;
        config.output_framerate = FRAMERATE;
        config.whip_whep_enable = false;
        config.rtmp_enable = false;

        let ctx = GraphicsContext::new(Default::default()).unwrap();
        let (device, queue) = (ctx.device.clone(), ctx.queue.clone());
        let pipeline = Pipeline::new(PipelineOptions {
            wgpu_options: PipelineWgpuOptions::Context(ctx),
            ..pipeline_options_from_config(&config, &None)
        })
        .unwrap();
        Self {
            pipeline: Arc::new(Mutex::new(pipeline)),
            device,
            queue,
        }
    }

    fn register_output(
        &self,
        output_id: &str,
        scene: OutputScene,
    ) -> Result<OutputFrames<'_>, RegisterOutputError> {
        let options = RegisterRawDataOutputOptions {
            output_options: RawDataOutputOptions {
                video: Some(RawDataOutputVideoOptions {
                    resolution: RESOLUTION,
                }),
                audio: Some(RawDataOutputAudioOptions),
            },
            video: Some(RegisterOutputVideoOptions { initial: scene }),
            audio: Some(RegisterOutputAudioOptions {
                initial: AudioMixerConfig { inputs: vec![] },
                mixing_strategy: AudioMixingStrategy::SumClip,
                channels: AudioChannels::Stereo,
            }),
        };
        let receiver = Pipeline::register_raw_data_output(
            &self.pipeline,
            OutputId(output_id.into()),
            options,
        )?;

        // Nothing reads audio, but output can't block on it.
        let audio = receiver.audio.unwrap();
        thread::spawn(move || audio.iter().for_each(drop));

        Ok(OutputFrames {
            pipeline: self,
            receiver: receiver.video.unwrap(),
        })
    }

    fn update_output(&self, output_id: &str, scene: OutputScene) -> Result<(), UpdateOutputError> {
        self.pipeline.lock().unwrap().update_output(
            OutputId(output_id.into()),
            Some(scene),
            Some(AudioMixerConfig { inputs: vec![] }),
        )
    }

    /// Registers an input and sends `frame_count` frames in a single color. The input ends
    /// after the last frame.
    fn register_input(&self, input_id: &str, offset: Duration, color: Color, frame_count: u32) {
        let sender = Pipeline::register_raw_data_input(
            &self.pipeline,
            InputId(input_id.into()),
            RawDataInputOptions {
                video: true,
                audio: false,
                required: true,
                offset: QueueTrackGroupOffset::FromStart(offset),
            },
        )
        .unwrap();
        let sender = sender.video.unwrap();
        thread::spawn(move || {
            for index in 0..frame_count {
                let pts = Timestamp::from_nanos(
                    index as i64 * 1_000_000_000 * FRAMERATE.den as i64 / FRAMERATE.num as i64,
                );
                if sender.send(solid_color_frame(color, pts)).is_err() {
                    return;
                }
            }
        });
    }

    fn start(&self) {
        Pipeline::start(&self.pipeline);
    }
}

struct OutputFrames<'a> {
    pipeline: &'a TestPipeline,
    receiver: Receiver<PipelineEvent<Frame>>,
}

impl OutputFrames<'_> {
    /// RGBA data of the next frame.
    fn next_frame(&self) -> Bytes {
        let event = self.receiver.recv_timeout(Duration::from_secs(20)).unwrap();
        let PipelineEvent::Data(frame) = event else {
            panic!("output ended");
        };
        let FrameData::Rgba8UnormWgpuTexture(texture) = frame.data else {
            panic!("unexpected frame format");
        };
        read_rgba_texture(&self.pipeline.device, &self.pipeline.queue, &texture)
    }

    /// Color in the middle of each of the next `count` frames.
    fn next_colors(&self, count: usize) -> Vec<Color> {
        (0..count)
            .map(|_| {
                color_at(
                    &self.next_frame(),
                    RESOLUTION.width / 2,
                    RESOLUTION.height / 2,
                )
            })
            .collect()
    }

    /// Reads frames until one has the `color` in the middle. Updates requested with API are
    /// not synchronized with frames, the change can show up a few frames later.
    #[track_caller]
    fn wait_for_color(&self, color: Color) {
        let colors = self.next_colors(300);
        assert!(colors.contains(&color), "no {color:?} frame in {colors:?}");
    }
}

fn solid_color_frame(color: Color, pts: Timestamp) -> Frame {
    // BT.709 limited range
    let (y, u, v) = match color {
        Color::Red => (63, 102, 240),
        Color::Green => (173, 42, 26),
        Color::Blue => (32, 240, 118),
        Color::Other => (128, 128, 128),
    };
    let pixels = RESOLUTION.width * RESOLUTION.height;
    Frame {
        data: FrameData::PlanarYuv420(YuvPlanes {
            y_plane: vec![y; pixels].into(),
            u_plane: vec![u; pixels / 4].into(),
            v_plane: vec![v; pixels / 4].into(),
        }),
        resolution: RESOLUTION,
        pts,
    }
}

fn color_at(frame: &Bytes, x: usize, y: usize) -> Color {
    let offset = (y * RESOLUTION.width + x) * 4;
    let (r, g, b) = (frame[offset], frame[offset + 1], frame[offset + 2]);
    match (r > 160, g > 160, b > 160) {
        (true, false, false) if g < 90 && b < 90 => Color::Red,
        (false, true, false) if r < 90 && b < 90 => Color::Green,
        (false, false, true) if r < 90 && g < 90 => Color::Blue,
        _ => Color::Other,
    }
}

fn script(source: &str, props: serde_json::Value) -> OutputScene {
    OutputScene::Script(ScriptSpec {
        source: source.into(),
        props,
    })
}

fn static_scene(color: RGBAColor) -> OutputScene {
    Scene::ComponentTree(Component::View(ViewComponent {
        background_color: color,
        ..Default::default()
    }))
    .into()
}

/// Index of the first frame of every range of frames with the same color.
fn color_changes(colors: &[Color]) -> Vec<(usize, Color)> {
    let mut changes: Vec<(usize, Color)> = vec![];
    for (index, color) in colors.iter().enumerate() {
        if changes.last().map(|(_, last)| last) != Some(color) {
            changes.push((index, *color));
        }
    }
    changes
}

#[test]
fn script_reacts_to_inputs_and_timers_in_sync_with_frames() {
    const SCRIPT: &str = r##"
        local background = "#0000FFFF"
        smelter.at(3000, function()
            background = "#00FF00FF"
        end)

        return function()
            for _, input in smelter.inputs() do
                if input.video == "playing" then
                    return Rescaler { mode = "fill", InputStream { input_id = input.id } }
                end
            end
            return View { background_color = background }
        end
    "##;

    let pipeline = TestPipeline::new();
    let frames = pipeline
        .register_output("output_1", script(SCRIPT, serde_json::Value::Null))
        .unwrap();
    // Plays frames 30-59 (1s-2s) and ends.
    pipeline.register_input("input_1", Duration::from_secs(1), Color::Red, 30);
    pipeline.start();

    let colors = frames.next_colors(120);
    assert_eq!(
        color_changes(&colors),
        [
            // nothing is playing
            (0, Color::Blue),
            // first frame of the input
            (30, Color::Red),
            // first frame after the input ended
            (60, Color::Blue),
            // timer at 3000ms
            (90, Color::Green),
        ]
    );
}

#[test]
fn measured_components_and_input_registered_after_start() {
    // Green box has the size of the text. It is replaced with the input when it is playing.
    // Red box has the size of the image divided by (6, 4).
    const SCRIPT: &str = r##"
        local text = Text { text = "Hello", font_size = 80, color = "#00FF00FF" }
        local size = smelter.measure(text)
        local image_size = smelter.measure(Image { image_id = "image" })
        local scaled_image_size = smelter.measure(Image { image_id = "image", width = 333 })
        assert(scaled_image_size.width == 333 and scaled_image_size.height == 262)

        local playing = {}
        smelter.on("video_input_playing", function(input)
            playing[input.id] = { width = input.width, height = input.height }
        end)
        smelter.on("video_input_eos", function(input)
            playing[input.id] = nil
        end)

        return function()
            local input_id, resolution = next(playing)
            return View {
                background_color = "#0000FFFF",
                input_id and View {
                    top = 0, left = 0, width = resolution.width / 2, height = resolution.height / 2,
                    Rescaler { InputStream { input_id = input_id } },
                },
                not input_id and View {
                    top = 0, left = 0, width = size.width, height = size.height,
                    background_color = "#00FF00FF",
                },
                View {
                    bottom = 0, right = 0,
                    width = image_size.width / 6, height = image_size.height / 4,
                    background_color = "#FF0000FF",
                },
            }
        end
    "##;

    let pipeline = TestPipeline::new();
    // 666x524
    let image = ImageSpec {
        src: ImageSource::LocalPath {
            path: integration_tests_root().join("assets/image.svg").into(),
        },
        image_type: ImageType::Svg,
    };
    Pipeline::register_renderer(
        &pipeline.pipeline,
        RendererId("image".into()),
        RendererSpec::Image(image),
    )
    .unwrap();
    let frames = pipeline
        .register_output("output_1", script(SCRIPT, serde_json::Value::Null))
        .unwrap();
    pipeline.start();

    // Size of the green box is the size of the text.
    let frame = frames.next_frame();
    let width = (0..RESOLUTION.width)
        .take_while(|x| color_at(&frame, *x, 2) == Color::Green)
        .count();
    let height = (0..RESOLUTION.height)
        .take_while(|y| color_at(&frame, 2, *y) == Color::Green)
        .count();
    assert!((100..400).contains(&width), "text width {width}");
    assert!((60..120).contains(&height), "text height {height}");
    assert_eq!(color_at(&frame, width + 2, 2), Color::Blue);

    // Size of the red box is based on the size of the image.
    let width = (0..RESOLUTION.width)
        .rev()
        .take_while(|x| color_at(&frame, *x, RESOLUTION.height - 2) == Color::Red)
        .count();
    let height = (0..RESOLUTION.height)
        .rev()
        .take_while(|y| color_at(&frame, RESOLUTION.width - 2, *y) == Color::Red)
        .count();
    assert_eq!((width, height), (666 / 6, 524 / 4));

    // Input shows up in the top left quarter and disappears after the last frame. Sampled
    // pixel is in the corner of that quarter, outside of the box.
    pipeline.register_input("input_1", Duration::ZERO, Color::Red, 20);
    let mut colors = vec![];
    for _ in 0..300 {
        let frame = frames.next_frame();
        colors.push((
            color_at(
                &frame,
                RESOLUTION.width / 2 - 10,
                RESOLUTION.height / 2 - 10,
            ),
            color_at(&frame, RESOLUTION.width * 3 / 4, RESOLUTION.height * 3 / 4),
        ));
    }
    let red_frames = colors
        .iter()
        .filter(|(top_left, _)| *top_left == Color::Red)
        .count();
    // Input registered while the queue is running loses frames that are already in the past.
    assert!((1..=20).contains(&red_frames), "{red_frames} red frames");
    assert_eq!(colors.last().unwrap().0, Color::Blue);
    assert!(
        colors
            .iter()
            .all(|(_, bottom_right)| *bottom_right == Color::Blue)
    );
}

#[test]
fn update_replaces_props_or_script() {
    // Red until props are updated for the first time, then the color from props.
    const SCRIPT: &str = r##"
        local updates = 0
        smelter.on("props", function()
            updates += 1
        end)

        return function()
            local color = if updates == 0 then "#FF0000FF" else smelter.props().color
            local audio = { inputs = { { input_id = "input_1", volume = 0.5 } } }
            return View { background_color = color }, audio
        end
    "##;
    let blue = serde_json::json!({ "color": "#0000FFFF" });
    let green = serde_json::json!({ "color": "#00FF00FF" });

    let pipeline = TestPipeline::new();
    let frames = pipeline
        .register_output("output_1", script(SCRIPT, blue.clone()))
        .unwrap();
    pipeline.start();
    frames.wait_for_color(Color::Red);

    // The same source, so the script keeps its state.
    pipeline
        .update_output("output_1", script(SCRIPT, green.clone()))
        .unwrap();
    frames.wait_for_color(Color::Green);

    // Different source, so the script starts from the beginning.
    let changed_script = format!("{SCRIPT}\n-- changed");
    pipeline
        .update_output("output_1", script(&changed_script, blue.clone()))
        .unwrap();
    frames.wait_for_color(Color::Red);

    // Invalid script is rejected and the old one keeps running.
    let err = pipeline
        .update_output("output_1", script("return View {", blue.clone()))
        .unwrap_err();
    assert!(
        matches!(err, UpdateOutputError::ScriptError(_, ScriptError::Load(_))),
        "{err:?}"
    );
    let err = pipeline
        .update_output(
            "output_1",
            script("return View { colour = 1 }", blue.clone()),
        )
        .unwrap_err();
    assert!(
        matches!(
            err,
            UpdateOutputError::ScriptError(_, ScriptError::InvalidScene(_))
        ),
        "{err:?}"
    );
    pipeline
        .update_output("output_1", script(&changed_script, green))
        .unwrap();
    frames.wait_for_color(Color::Green);

    // Static scene stops the script.
    pipeline
        .update_output("output_1", static_scene(RGBAColor(0, 0, 255, 255)))
        .unwrap();
    frames.wait_for_color(Color::Blue);
    assert!(frames.next_colors(100).iter().all(|c| *c == Color::Blue));

    // Script can be started again.
    pipeline
        .update_output("output_1", script(SCRIPT, blue))
        .unwrap();
    frames.wait_for_color(Color::Red);
}

#[test]
fn output_with_invalid_script_is_not_registered() {
    let pipeline = TestPipeline::new();

    let result = pipeline.register_output(
        "output_1",
        script("local x = nil\nreturn x.y", serde_json::Value::Null),
    );
    let err = result.err().unwrap();
    assert!(
        matches!(
            err,
            RegisterOutputError::ScriptError(_, ScriptError::Runtime(_))
        ),
        "{err:?}"
    );
    assert_eq!(pipeline.pipeline.lock().unwrap().outputs().count(), 0);

    // the same id can be used again
    let source = r##"return View { background_color = "#00FF00FF" }"##;
    let frames = pipeline
        .register_output("output_1", script(source, serde_json::Value::Null))
        .unwrap();
    pipeline.start();
    frames.wait_for_color(Color::Green);
}

#[test]
fn script_that_exceeds_time_limit_keeps_last_scene() {
    const SCRIPT: &str = r##"
        local color = "#FF0000FF"
        smelter.at(1000, function()
            color = "#00FF00FF"
        end)
        smelter.at(2000, function()
            while true do end
        end)
        smelter.at(3000, function()
            color = "#0000FFFF"
        end)

        return function()
            return View { background_color = color }
        end
    "##;

    let pipeline = TestPipeline::new();
    let events = pipeline
        .pipeline
        .lock()
        .unwrap()
        .subscribe_pipeline_events();
    let frames = pipeline
        .register_output("output_1", script(SCRIPT, serde_json::Value::Null))
        .unwrap();
    pipeline.start();

    let colors = frames.next_colors(150);
    assert_eq!(
        color_changes(&colors),
        [(0, Color::Red), (30, Color::Green)]
    );

    let error = events.try_iter().find_map(|event| match event {
        smelter_core::event::Event::OutputError { err, .. } => Some(err.to_string()),
        _ => None,
    });
    assert!(
        error.as_ref().is_some_and(|err| err.contains("terminated")),
        "{error:?}"
    );

    // A terminated script can be started again with the same source. It renders the first
    // scene and it is terminated again by the timer that already passed.
    pipeline
        .update_output("output_1", script(SCRIPT, serde_json::Value::Null))
        .unwrap();
    frames.wait_for_color(Color::Red);
}
