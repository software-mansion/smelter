use std::{sync::Arc, time::Duration};

use smelter_core::{
    AudioMixerInputConfig, Script, ScriptError, ScriptEvent, ScriptEventKind, ScriptFrame,
    ScriptHost, ScriptInput, ScriptOutputCtx, ScriptRuntime, ScriptSpec, ScriptUpdate, Timestamp,
    TrackState,
};
use smelter_render::{
    Framerate, InputId, OutputId, RendererId, Resolution,
    scene::{Component, ComponentSize, Position, RGBAColor, SceneError, Size},
};

use crate::{LuaRuntimeOptions, LuaScriptRuntime, convert::MAX_SCENE_DEPTH};

/// Text is 10px wide per character, image "logo" is 200x100.
struct TestHost;

impl ScriptHost for TestHost {
    fn measure_component(&self, component: Component) -> Result<ComponentSize, SceneError> {
        let (width, height) = match component {
            Component::Text(text) => (Some(text.text.len() as f32 * 10.0), Some(text.line_height)),
            Component::Image(image) if &*image.image_id.0 == "logo" => (Some(200.0), Some(100.0)),
            Component::Image(image) => return Err(SceneError::ImageNotFound(image.image_id)),
            Component::View(view) => match view.position {
                Position::Static { width, height } => (width, height),
                Position::Absolute(position) => (position.width, position.height),
            },
            _ => (None, None),
        };
        Ok(ComponentSize { width, height })
    }
}

struct TestScript {
    script: Box<dyn Script>,
    inputs: Vec<ScriptInput>,
}

impl TestScript {
    fn start(source: &str) -> Result<(Self, ScriptUpdate), ScriptError> {
        Self::start_with(source, serde_json::Value::Null, test_options(), vec![])
    }

    fn start_with(
        source: &str,
        props: serde_json::Value,
        options: LuaRuntimeOptions,
        inputs: Vec<ScriptInput>,
    ) -> Result<(Self, ScriptUpdate), ScriptError> {
        let spec = ScriptSpec {
            source: source.into(),
            props,
        };
        let ctx = ScriptOutputCtx {
            output_id: OutputId("output_1".into()),
            resolution: Resolution {
                width: 1280,
                height: 720,
            },
            framerate: Framerate { num: 30, den: 1 },
            host: Arc::new(TestHost),
        };
        let script = LuaScriptRuntime::new(options).load(&spec, ctx)?;
        let mut script = Self { script, inputs };
        let update = script.update(0, &[])?;
        Ok((script, update))
    }

    fn update(
        &mut self,
        time_ms: i64,
        events: &[(&str, ScriptEventKind)],
    ) -> Result<ScriptUpdate, ScriptError> {
        let events: Vec<_> = events
            .iter()
            .map(|(input_id, kind)| ScriptEvent {
                input_id: InputId((*input_id).into()),
                kind: *kind,
            })
            .collect();
        self.script.update(&ScriptFrame {
            pts: Timestamp::from_millis(time_ms),
            inputs: &self.inputs,
            events: &events,
        })
    }

    /// Updates state of the input the same way pipeline does, then runs the script.
    fn input_event(
        &mut self,
        time_ms: i64,
        input_id: &str,
        kind: ScriptEventKind,
    ) -> Result<ScriptUpdate, ScriptError> {
        use ScriptEventKind as Kind;

        let position = self
            .inputs
            .iter()
            .position(|input| &*input.input_id.0 == input_id);
        match (kind, position) {
            (Kind::InputRegistered, None) => self.inputs.push(input(input_id, None)),
            (Kind::InputUnregistered, Some(index)) => {
                self.inputs.remove(index);
            }
            (Kind::VideoPlaying, Some(index)) => {
                self.inputs[index].video = Some(TrackState::Playing);
                self.inputs[index].resolution = Some(Resolution {
                    width: 640,
                    height: 360,
                });
            }
            (Kind::VideoEos, Some(index)) => self.inputs[index].video = Some(TrackState::Finished),
            (Kind::AudioPlaying, Some(index)) => {
                self.inputs[index].audio = Some(TrackState::Playing)
            }
            (kind, position) => panic!("unsupported event {kind:?} {position:?}"),
        }
        self.update(time_ms, &[(input_id, kind)])
    }
}

fn input(input_id: &str, video: Option<TrackState>) -> ScriptInput {
    ScriptInput {
        input_id: InputId(input_id.into()),
        video,
        audio: None,
        resolution: video.map(|_| Resolution {
            width: 640,
            height: 360,
        }),
    }
}

/// Luau is not optimized in debug builds, so defaults are too strict for tests.
fn test_options() -> LuaRuntimeOptions {
    LuaRuntimeOptions {
        memory_limit: 8 * 1024 * 1024,
        start_time_limit: Duration::from_millis(500),
        update_time_limit: Duration::from_millis(200),
    }
}

/// Texts in the scene in depth-first order.
fn texts(component: &Component) -> Vec<String> {
    match component {
        Component::Text(text) => vec![text.text.to_string()],
        Component::View(view) => view.children.iter().flat_map(texts).collect(),
        Component::Tiles(tiles) => tiles.children.iter().flat_map(texts).collect(),
        Component::Rescaler(rescaler) => texts(&rescaler.child),
        _ => vec![],
    }
}

fn scene_texts(update: &ScriptUpdate) -> Vec<String> {
    texts(update.video.as_ref().expect("script did not render"))
}

/// Ids of inputs used in the scene in depth-first order.
fn input_streams(component: &Component) -> Vec<String> {
    match component {
        Component::InputStream(input) => vec![input.input_id.0.to_string()],
        Component::View(view) => view.children.iter().flat_map(input_streams).collect(),
        Component::Tiles(tiles) => tiles.children.iter().flat_map(input_streams).collect(),
        Component::Rescaler(rescaler) => input_streams(&rescaler.child),
        _ => vec![],
    }
}

#[track_caller]
fn assert_no_errors(update: &ScriptUpdate) {
    assert!(update.errors.is_empty(), "{:?}", update.errors);
}

#[track_caller]
fn error_message(err: &ScriptError) -> &str {
    match err {
        ScriptError::Load(msg)
        | ScriptError::Runtime(msg)
        | ScriptError::InvalidScene(msg)
        | ScriptError::Terminated(msg) => msg,
        ScriptError::RuntimeNotAvailable => panic!("unexpected error {err}"),
    }
}

#[test]
fn script_returns_static_component() {
    let (mut script, update) = TestScript::start(
        r##"
        return View {
            background_color = "#FF0000FF",
            Text { "Hello", font_size = 20 },
            Text { text = 123, font_size = 20 },
        }
        "##,
    )
    .unwrap();

    assert_no_errors(&update);
    let Some(Component::View(view)) = &update.video else {
        panic!("expected view {update:?}");
    };
    assert_eq!(view.background_color, RGBAColor(255, 0, 0, 255));
    assert_eq!(scene_texts(&update), ["Hello", "123"]);

    // nothing to do without events
    let update = script.update(1000, &[]).unwrap();
    assert!(update.video.is_none());
}

#[test]
fn children_can_be_lists_or_skipped() {
    let (_, update) = TestScript::start(
        r##"
        local list = { Text { "b", font_size = 10 }, { Text { "c", font_size = 10 } } }
        local missing = nil
        -- positional values with holes that are not a part of a table constructor
        local with_holes = { background_color = "#000000FF" }
        with_holes[2] = Text { "f", font_size = 10 }
        with_holes[5] = Text { "g", font_size = 10 }
        return View {
            children = { Text { "a", font_size = 10 } },
            missing,
            list,
            false,
            1 > 2 and Text { "skipped", font_size = 10 },
            Rescaler { Text { "d", font_size = 10 } },
            Tiles { children = {} },
            missing,
            { type = "text", text = "e", font_size = 10 },
            View(with_holes),
            missing,
        }
        "##,
    )
    .unwrap();

    assert_no_errors(&update);
    assert_eq!(scene_texts(&update), ["a", "b", "c", "d", "e", "f", "g"]);
}

#[test]
fn fields_use_http_api_types() {
    let (_, update) = TestScript::start(
        r##"
        return View {
            width = 100.5,
            height = 50,
            top = 10,
            left = 20,
            transition = { duration_ms = 300, easing_function = { function_name = "bounce" } },
            box_shadow = { { offset_x = 1, blur_radius = 2, color = "#00000080" } },
        }
        "##,
    )
    .unwrap();

    let Some(Component::View(view)) = &update.video else {
        panic!("expected view {update:?}");
    };
    let Position::Absolute(position) = view.position else {
        panic!("expected absolute position {view:?}");
    };
    assert_eq!((position.width, position.height), (Some(100.5), Some(50.0)));
    assert_eq!(
        view.transition.unwrap().duration,
        Duration::from_millis(300)
    );
    assert_eq!(view.box_shadow.len(), 1);
}

#[test]
fn scene_follows_input_state() {
    let (mut script, update) = TestScript::start(
        r##"
        return function()
            local tiles = {}
            for _, input in smelter.inputs() do
                if input.video == "playing" then
                    table.insert(tiles, InputStream { input_id = input.id })
                end
            end
            return Tiles { id = "tiles", children = tiles }
        end
        "##,
    )
    .unwrap();
    assert_eq!(
        input_streams(update.video.as_ref().unwrap()),
        [] as [&str; 0]
    );

    use ScriptEventKind::*;
    let update = script.input_event(0, "a", InputRegistered).unwrap();
    assert_eq!(
        input_streams(update.video.as_ref().unwrap()),
        [] as [&str; 0]
    );

    let update = script.input_event(33, "a", VideoPlaying).unwrap();
    assert_eq!(input_streams(update.video.as_ref().unwrap()), ["a"]);

    script.input_event(66, "b", InputRegistered).unwrap();
    let update = script.input_event(66, "b", VideoPlaying).unwrap();
    assert_eq!(input_streams(update.video.as_ref().unwrap()), ["a", "b"]);

    let update = script.input_event(100, "a", VideoEos).unwrap();
    assert_eq!(input_streams(update.video.as_ref().unwrap()), ["b"]);

    let update = script.input_event(133, "b", InputUnregistered).unwrap();
    assert_eq!(
        input_streams(update.video.as_ref().unwrap()),
        [] as [&str; 0]
    );
}

#[test]
fn script_started_with_inputs_sees_them() {
    let (_, update) = TestScript::start_with(
        r##"
        local count = #smelter.inputs()
        return function()
            local input = smelter.input("a")
            return Text { `{count} {input.id} {input.video} {input.width}x{input.height}`, font_size = 10 }
        end
        "##,
        serde_json::Value::Null,
        test_options(),
        vec![input("a", Some(TrackState::Playing))],
    )
    .unwrap();

    assert_eq!(scene_texts(&update), ["1 a playing 640x360"]);
}

#[test]
fn event_handlers() {
    let (mut script, _) = TestScript::start(
        r##"
        local log = {}
        local function record(name)
            smelter.on(name, function(input)
                table.insert(log, `{name}:{input.id}:{input.video}`)
            end)
        end
        record("input_registered")
        record("video_input_playing")
        record("video_input_eos")
        record("input_unregistered")

        local once
        once = smelter.on("audio_input_playing", function(input)
            table.insert(log, "audio once")
            once:cancel()
        end)

        return function()
            return Text { table.concat(log, " "), font_size = 10 }
        end
        "##,
    )
    .unwrap();

    use ScriptEventKind::*;
    script.input_event(0, "a", InputRegistered).unwrap();
    script.input_event(0, "a", VideoPlaying).unwrap();
    script.input_event(0, "a", AudioPlaying).unwrap();
    script.input_event(0, "a", AudioPlaying).unwrap();
    script.input_event(0, "a", VideoEos).unwrap();
    let update = script.input_event(0, "a", InputUnregistered).unwrap();

    assert_no_errors(&update);
    assert_eq!(
        scene_texts(&update),
        [
            "input_registered:a:nil video_input_playing:a:playing audio once \
            video_input_eos:a:finished input_unregistered:a:finished"
        ]
    );
}

#[test]
fn timers() {
    let (mut script, update) = TestScript::start(
        r##"
        local log = {}
        local function record(name)
            return function()
                table.insert(log, `{name}@{smelter.time()}`)
            end
        end
        smelter.after(100, record("after"))
        smelter.at(250, record("at"))
        local every = smelter.every(100, record("every"))
        smelter.after(450, function() every:cancel() end)
        smelter.after(50, record("cancelled")):cancel()

        return function()
            return Text { table.concat(log, " "), font_size = 10 }
        end
        "##,
    )
    .unwrap();
    assert_eq!(scene_texts(&update), [""]);

    // not due yet
    assert!(script.update(99, &[]).unwrap().video.is_none());

    let update = script.update(100, &[]).unwrap();
    assert_eq!(scene_texts(&update), ["after@100 every@100"]);

    // "every" fires only once even if more than one interval passed
    let update = script.update(330, &[]).unwrap();
    assert_eq!(
        scene_texts(&update),
        ["after@100 every@100 every@330 at@330"]
    );

    // interval is not shifted by a late call
    let update = script.update(400, &[]).unwrap();
    assert_eq!(
        scene_texts(&update),
        ["after@100 every@100 every@330 at@330 every@400"]
    );

    // timer from 450 cancels "every" before it runs
    let update = script.update(500, &[]).unwrap();
    assert_eq!(
        scene_texts(&update),
        ["after@100 every@100 every@330 at@330 every@400"]
    );
    assert!(script.update(1000, &[]).unwrap().video.is_none());
}

#[test]
fn timer_created_in_callback_runs_on_later_update() {
    let (mut script, _) = TestScript::start(
        r##"
        local count = 0
        local function schedule()
            smelter.after(0, function()
                count += 1
                schedule()
            end)
        end
        schedule()
        return function()
            return Text { count, font_size = 10 }
        end
        "##,
    )
    .unwrap();

    assert_eq!(scene_texts(&script.update(0, &[]).unwrap()), ["1"]);
    assert_eq!(scene_texts(&script.update(0, &[]).unwrap()), ["2"]);
}

#[test]
fn stack_overflow_is_a_regular_error() {
    let (mut script, _) = TestScript::start(
        r##"
        smelter.after(100, function()
            local function recurse() return 1 + recurse() end
            recurse()
        end)
        return function()
            return Text { smelter.time(), font_size = 10 }
        end
        "##,
    )
    .unwrap();

    let update = script.update(100, &[]).unwrap();
    assert_eq!(scene_texts(&update), ["100"]);
    assert_eq!(update.errors.len(), 1);
    assert!(
        error_message(&update.errors[0]).contains("stack overflow"),
        "{:?}",
        update.errors
    );
}

#[test]
fn frame_handler_runs_on_every_update() {
    let (mut script, _) = TestScript::start(
        r##"
        local frames = 0
        local subscription = smelter.on("frame", function(time)
            frames += 1
        end)
        smelter.after(100, function() subscription:cancel() end)
        return function()
            return Text { frames, font_size = 10 }
        end
        "##,
    )
    .unwrap();

    assert_eq!(scene_texts(&script.update(33, &[]).unwrap()), ["1"]);
    assert_eq!(scene_texts(&script.update(66, &[]).unwrap()), ["2"]);
    // cancelled by the timer before frame handlers run
    assert_eq!(scene_texts(&script.update(100, &[]).unwrap()), ["2"]);
    assert!(script.update(133, &[]).unwrap().video.is_none());
}

#[test]
fn measure_components() {
    let (_, update) = TestScript::start(
        r##"
        local text = smelter.measure(Text { "12345", font_size = 20 })
        local image = smelter.measure(Image { image_id = "logo" })
        local view = smelter.measure(View { width = 300 })
        local ok, err = pcall(smelter.measure, Image { image_id = "unknown" })
        return View {
            Text { `{text.width}x{text.height}`, font_size = 10 },
            Text { `{image.width}x{image.height}`, font_size = 10 },
            Text { `{view.width}x{view.height}`, font_size = 10 },
            Text { `{ok} {err}`, font_size = 10 },
            Text { `{smelter.output.id} {smelter.output.width}x{smelter.output.height} {smelter.output.framerate}`, font_size = 10 },
        }
        "##,
    )
    .unwrap();

    assert_eq!(
        scene_texts(&update),
        [
            "50x20",
            "200x100",
            "300xnil",
            "false smelter.measure failed: Image \"unknown\" does not exist. You have to \
            register it first before using it in the scene definition.",
            "output_1 1280x720 30",
        ]
    );
}

#[test]
fn props() {
    let (mut script, update) = TestScript::start_with(
        r##"
        local changes = 0
        smelter.on("props", function(props, previous)
            changes += 1
            assert(previous.title == "first")
        end)
        local ok = pcall(function() smelter.props().title = "modified" end)
        assert(not ok, "props are read-only")
        return function()
            local props = smelter.props()
            return Text { `{props.title} {#props.list} {props.nested.value} {props.missing} {changes}`, font_size = 10 }
        end
        "##,
        serde_json::json!({ "title": "first", "list": [1, 2, 3], "nested": { "value": true }, "missing": null }),
        test_options(),
        vec![],
    )
    .unwrap();
    assert_eq!(scene_texts(&update), ["first 3 true nil 0"]);

    script.script.set_props(
        serde_json::json!({ "title": "second", "list": [], "nested": { "value": false } }),
    );
    let update = script.update(0, &[]).unwrap();
    assert_no_errors(&update);
    assert_eq!(scene_texts(&update), ["second 0 false nil 1"]);
}

#[test]
fn audio_config() {
    let (_, update) = TestScript::start(
        r##"
        return function()
            return View {}, { inputs = { { input_id = "a", volume = 0.5 }, { input_id = "b" } } }
        end
        "##,
    )
    .unwrap();

    assert_no_errors(&update);
    assert_eq!(
        update.audio.unwrap().inputs,
        [
            AudioMixerInputConfig {
                input_id: InputId("a".into()),
                volume: 0.5
            },
            AudioMixerInputConfig {
                input_id: InputId("b".into()),
                volume: 1.0
            },
        ]
    );
}

#[test]
fn integer_fields() {
    let (_, update) = TestScript::start(
        r##"
        return Shader {
            shader_id = "shader",
            resolution = { width = 640, height = 360 },
            shader_param = { type = "struct", value = {
                { field_name = "count", type = "u32", value = 3 },
                { field_name = "ratio", type = "f32", value = 0.5 },
            } },
            InputStream { input_id = "a" },
        }
        "##,
    )
    .unwrap();

    assert_no_errors(&update);
    let Some(Component::Shader(shader)) = &update.video else {
        panic!("expected shader {update:?}");
    };
    assert_eq!(shader.shader_id, RendererId("shader".into()));
    assert_eq!(
        shader.size,
        Size {
            width: 640.0,
            height: 360.0
        }
    );
    assert_eq!(shader.children.len(), 1);
}

#[test]
fn syntax_error_is_reported_with_line() {
    let err = TestScript::start("local x = 1\nreturn View {")
        .err()
        .unwrap();
    assert!(matches!(err, ScriptError::Load(_)), "{err:?}");
    assert!(error_message(&err).contains("script:2:"), "{err}");
}

#[test]
fn runtime_error_on_start_is_reported_with_line() {
    let err = TestScript::start("local x = nil\nreturn x.field")
        .err()
        .unwrap();
    assert!(matches!(err, ScriptError::Runtime(_)), "{err:?}");
    assert!(error_message(&err).contains("script:2:"), "{err}");

    let err = TestScript::start("return function()\n  error('render failed')\nend")
        .err()
        .unwrap();
    assert!(
        error_message(&err).contains("script:2: render failed"),
        "{err}"
    );

    let err = TestScript::start("return 1").err().unwrap();
    assert!(
        error_message(&err).contains("script has to return a render function or a component"),
        "{err}"
    );
}

#[test]
fn invalid_scene_error_points_to_component() {
    let err = TestScript::start(
        r##"
        return View {
            Text { "valid", font_size = 10 },
            Rescaler { View { Text { "invalid", font_size = 10, colour = "#FFFFFFFF" } } },
        }
        "##,
    )
    .err()
    .unwrap();

    assert!(matches!(err, ScriptError::InvalidScene(_)), "{err:?}");
    let msg = error_message(&err);
    assert!(
        msg.contains("`root.children[2].child.children[1]`"),
        "{msg}"
    );
    assert!(msg.contains("unknown field `colour`"), "{msg}");

    let err = TestScript::start(r##"return View { top = 10 }"##)
        .err()
        .unwrap();
    assert!(
        error_message(&err).contains("\"left\" or \"right\""),
        "{err}"
    );

    let err = TestScript::start(r##"return View { "text" }"##)
        .err()
        .unwrap();
    assert!(
        error_message(&err).contains("script:1: View: a child has to be a component"),
        "{err}"
    );
}

#[test]
fn error_values_that_are_not_strings() {
    let (mut script, _) = TestScript::start(
        r##"
        smelter.after(100, function()
            error({ code = 1 })
        end)
        smelter.after(100, function()
            error(setmetatable({}, { __tostring = function() error("tostring failed") end }))
        end)
        smelter.after(100, function()
            error(setmetatable({}, { __tostring = function() return "custom message" end }))
        end)
        return function()
            return Text { smelter.time(), font_size = 10 }
        end
        "##,
    )
    .unwrap();

    let update = script.update(100, &[]).unwrap();
    assert_eq!(scene_texts(&update), ["100"]);
    let errors: Vec<_> = update.errors.iter().map(error_message).collect();
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(errors[0].starts_with("table: "), "{errors:?}");
    assert_eq!(errors[1], "error value can't be converted to a string");
    assert_eq!(errors[2], "custom message");
}

#[test]
fn handler_error_does_not_stop_script() {
    let (mut script, _) = TestScript::start(
        r##"
        local calls = 0
        smelter.on("input_registered", function(input)
            error("first handler failed")
        end)
        smelter.on("input_registered", function(input)
            calls += 1
        end)
        return function()
            return Text { calls, font_size = 10 }
        end
        "##,
    )
    .unwrap();

    let update = script
        .input_event(0, "a", ScriptEventKind::InputRegistered)
        .unwrap();
    assert_eq!(scene_texts(&update), ["1"]);
    assert_eq!(update.errors.len(), 1);
    assert!(
        error_message(&update.errors[0]).contains("script:4: first handler failed"),
        "{:?}",
        update.errors
    );
}

#[test]
fn render_error_keeps_script_running() {
    let (mut script, _) = TestScript::start(
        r##"
        return function()
            if #smelter.inputs() == 1 then
                error("render failed")
            end
            return Text { #smelter.inputs(), font_size = 10 }
        end
        "##,
    )
    .unwrap();

    let update = script
        .input_event(0, "a", ScriptEventKind::InputRegistered)
        .unwrap();
    assert!(update.video.is_none());
    assert_eq!(update.errors.len(), 1);

    let update = script
        .input_event(0, "b", ScriptEventKind::InputRegistered)
        .unwrap();
    assert_no_errors(&update);
    assert_eq!(scene_texts(&update), ["2"]);
}

#[test]
fn sandbox() {
    let (_, update) = TestScript::start(
        r##"
        local results = {}
        local function check(name, value)
            table.insert(results, `{name}={tostring(value)}`)
        end
        check("io", io)
        check("debug", debug)
        check("package", package)
        check("require", require)
        check("loadstring", loadstring)
        check("load", load)
        check("dofile", dofile)
        check("getfenv", getfenv)
        check("setfenv", setfenv)
        check("os.execute", os.execute)
        check("os.getenv", os.getenv)
        check("string.dump", string.dump)
        -- API and standard libraries can't be modified
        check("smelter", pcall(function() smelter.time = function() return 0 end end))
        check("output", pcall(function() smelter.output.width = 1 end))
        check("string", pcall(function() string.rep = nil end))
        check("metatable", pcall(function() getmetatable("").__index = {} end))
        -- globals of the script are separate from globals of the VM
        View = 1
        check("View", View)
        return Text { table.concat(results, " "), font_size = 10 }
        "##,
    )
    .unwrap();

    assert_eq!(
        scene_texts(&update),
        [
            "io=nil debug=nil package=nil require=nil loadstring=nil load=nil dofile=nil \
            getfenv=nil setfenv=nil os.execute=nil os.getenv=nil string.dump=nil smelter=false \
            output=false string=false metatable=false View=1"
        ]
    );
}

#[test]
fn infinite_loop_on_start_is_rejected() {
    let options = LuaRuntimeOptions {
        start_time_limit: Duration::from_millis(20),
        ..test_options()
    };
    let result = TestScript::start_with(
        "while true do end",
        serde_json::Value::Null,
        options,
        vec![],
    );
    let err = result.err().unwrap();
    assert!(matches!(err, ScriptError::Terminated(_)), "{err:?}");
}

#[test]
fn script_that_exceeds_time_limit_is_terminated() {
    let options = LuaRuntimeOptions {
        update_time_limit: Duration::from_millis(20),
        ..test_options()
    };
    let sources = [
        "smelter.after(100, function() while true do end end)",
        // error raised by the limit can be caught, but the script still can't continue
        "smelter.after(100, function() while true do pcall(function() while true do end end) end end)",
        // pattern matching that does not end in a reasonable time
        r#"smelter.after(100, function() string.find(string.rep("a", 5000), ".-.-.-.-.-.-.-.-x") end)"#,
        "smelter.after(100, function() coroutine.wrap(function() while true do end end)() end)",
    ];
    for source in sources {
        let source = format!(
            "{source}\nreturn function() return Text {{ smelter.time(), font_size = 10 }} end"
        );
        let (mut script, _) =
            TestScript::start_with(&source, serde_json::Value::Null, options, vec![]).unwrap();

        let err = script.update(100, &[]).err().unwrap();
        assert!(
            matches!(err, ScriptError::Terminated(_)),
            "{source}: {err:?}"
        );

        // terminated script does nothing
        let update = script
            .input_event(200, "a", ScriptEventKind::InputRegistered)
            .unwrap();
        assert!(update.video.is_none());
        assert_no_errors(&update);
    }
}

#[test]
fn memory_limit() {
    let (mut script, _) = TestScript::start(
        r##"
        local data = {}
        smelter.after(100, function()
            for i = 1, 1e9 do
                data[i] = string.rep("x", 1024)
            end
        end)
        smelter.after(200, function()
            data = nil
        end)
        return function()
            return Text { tostring(data ~= nil), font_size = 10 }
        end
        "##,
    )
    .unwrap();

    let update = script.update(100, &[]).unwrap();
    assert_eq!(update.errors.len(), 1, "{:?}", update.errors);
    assert!(
        error_message(&update.errors[0]).contains("not enough memory"),
        "{:?}",
        update.errors
    );

    // script still works after memory is released
    let update = script.update(200, &[]).unwrap();
    assert_no_errors(&update);
    assert_eq!(scene_texts(&update), ["false"]);
}

#[test]
fn recursive_scene_is_rejected() {
    let err = TestScript::start(
        r##"
        local view = View {}
        view.children = { view }
        return view
        "##,
    )
    .err()
    .unwrap();
    assert!(matches!(err, ScriptError::InvalidScene(_)), "{err:?}");
}

#[test]
fn scene_size_is_limited() {
    let cases = [
        // 2^40 components described with 41 tables
        (
            r##"
            local scene = Text { "x", font_size = 10 }
            for _ = 1, 40 do
                scene = View { scene, scene }
            end
            return scene
            "##,
            "values",
        ),
        (
            r##"return View { children = table.create(100000, Text { "x", font_size = 10 }) }"##,
            "values",
        ),
        (
            r##"return Text { text = string.rep("x", 1000000), font_size = 10 }"##,
            "bytes of text",
        ),
        (r##"return View { width = 0 / 0 }"##, "not finite"),
        (
            r##"return Text { "x", font_size = math.huge }"##,
            "not finite",
        ),
    ];
    for (source, expected) in cases {
        let err = TestScript::start(source).err().unwrap();
        assert!(matches!(err, ScriptError::InvalidScene(_)), "{err:?}");
        assert!(error_message(&err).contains(expected), "{err}");
    }

    // the same limits apply to components that are only measured
    let (_, update) = TestScript::start(
        r##"
        local scene = Text { "x", font_size = 10 }
        for _ = 1, 40 do
            scene = View { scene, scene }
        end
        local ok, err = pcall(smelter.measure, scene)
        return Text { `{ok} {err}`, font_size = 10 }
        "##,
    )
    .unwrap();
    let text = &scene_texts(&update)[0];
    assert!(
        text.starts_with("false") && text.contains("values"),
        "{text}"
    );
}

#[test]
fn bytecode_is_not_loaded() {
    let bytecode = mlua::chunk::Compiler::new().compile("return {}").unwrap();
    let source = String::from_utf8_lossy(&bytecode);
    let err = TestScript::start(&source).err().unwrap();
    assert!(matches!(err, ScriptError::Load(_)), "{err:?}");
}

#[test]
fn plain_string_search_is_limited() {
    let (_, update) = TestScript::start(
        r##"
        -- takes seconds without the limit
        local text = string.rep("a", 1000000)
        local searched = string.rep("a", 500000) .. "b"
        local find_ok, find_err = pcall(string.find, text, searched)
        local method_ok = pcall(function() return text:find(searched, 1, true) end)
        local split_ok = pcall(string.split, text, searched)

        local from, to = string.find("hello world", "wor")
        local parts = string.split("a,b,c", ",")
        local _, _, captured = ("key=value"):find("(%w+)=")
        return Text {
            `{find_ok} {find_err} {method_ok} {split_ok} {from} {to} {#parts} {captured}`,
            font_size = 10,
        }
        "##,
    )
    .unwrap();

    assert_eq!(
        scene_texts(&update),
        ["false string.find: arguments are too long false false 7 9 3 key"]
    );
}

#[test]
fn reported_errors_are_limited() {
    let (mut script, _) = TestScript::start(
        r##"
        smelter.after(100, function()
            error(string.rep("x", 4000000))
        end)
        smelter.after(100, function()
            error("\255\254 not a valid UTF-8")
        end)
        for _ = 1, 20 do
            smelter.after(100, function()
                error("failed")
            end)
        end
        return function()
            return Text { smelter.time(), font_size = 10 }
        end
        "##,
    )
    .unwrap();

    let update = script.update(100, &[]).unwrap();
    assert_eq!(scene_texts(&update), ["100"]);
    assert_eq!(update.errors.len(), 8, "{:?}", update.errors.len());
    assert!(error_message(&update.errors[0]).len() < 1100);
    assert!(
        error_message(&update.errors[1]).contains("not a valid UTF-8"),
        "{:?}",
        update.errors[1]
    );
}

#[test]
fn frame_handler_runs_for_frame_with_the_same_time_as_start() {
    let (mut script, _) = TestScript::start(
        r##"
        local frames = 0
        smelter.on("frame", function() frames += 1 end)
        return function()
            return Text { frames, font_size = 10 }
        end
        "##,
    )
    .unwrap();

    assert_eq!(scene_texts(&script.update(0, &[]).unwrap()), ["1"]);
    // called once per frame, even if the script is updated again for the same frame
    script.script.set_props(serde_json::json!({}));
    assert_eq!(scene_texts(&script.update(0, &[]).unwrap()), ["1"]);
    assert_eq!(scene_texts(&script.update(33, &[]).unwrap()), ["2"]);
}

#[test]
fn print_does_not_fail() {
    let (_, update) = TestScript::start(
        r##"
        for i = 1, 100 do
            print("message", i, nil, {})
        end
        return View {}
        "##,
    )
    .unwrap();
    assert_no_errors(&update);
}

#[test]
fn deeply_nested_scene() {
    const SOURCE: &str = r##"
        local scene = Text { "leaf", font_size = 10 }
        for _ = 1, smelter.props().depth do
            scene = View { scene }
        end
        return scene
    "##;
    let start = |depth: usize| {
        TestScript::start_with(
            SOURCE,
            serde_json::json!({ "depth": depth }),
            test_options(),
            vec![],
        )
    };

    let (_, update) = start(MAX_SCENE_DEPTH - 1).unwrap();
    assert_eq!(scene_texts(&update), ["leaf"]);

    for depth in [MAX_SCENE_DEPTH, 10_000] {
        let err = start(depth).err().unwrap();
        assert!(matches!(err, ScriptError::InvalidScene(_)), "{err:?}");
        assert!(error_message(&err).contains("nested"), "{err}");
    }
}

#[test]
fn sort_without_comparator_is_limited() {
    let (_, update) = TestScript::start(
        r##"
        local long = table.create(200000, 1)
        local without_comparator = pcall(table.sort, long)
        -- comparator is code of the script, so the time limit applies
        local short = { 3, 1, 2 }
        table.sort(short)
        table.sort(short, function(a, b) return a > b end)
        return Text { `{without_comparator} {table.concat(short, ",")}`, font_size = 10 }
        "##,
    )
    .unwrap();

    assert_eq!(scene_texts(&update), ["false 3,2,1"]);
}
