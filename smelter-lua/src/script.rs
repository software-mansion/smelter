use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use mlua::{Function, Lua, LuaOptions, StdLib, Table, Value, VmState, chunk::ChunkMode};
use smelter_core::{
    Script, ScriptError, ScriptEventKind, ScriptFrame, ScriptInput, ScriptOutputCtx, ScriptSpec,
    ScriptUpdate, TrackState,
};
use smelter_render::error::ErrorStack;
use tracing::{info, warn};

use crate::{
    LuaRuntimeOptions,
    budget::Budget,
    convert::{audio_from_lua, component_from_lua, error_message, json_to_lua},
};

const PRELUDE: &str = include_str!("prelude.luau");

/// Lines printed by a script during a single update above this limit are dropped.
const MAX_LOG_LINES_PER_UPDATE: u32 = 32;
const MAX_LOG_LINE_BYTES: usize = 4096;

pub(crate) struct LuaScript {
    options: LuaRuntimeOptions,
    /// `None` after the script was terminated.
    vm: Option<Vm>,
    /// Props that were not passed to the script yet.
    pending_props: Option<serde_json::Value>,
    /// Time of the next timer in milliseconds.
    next_wake_ms: f64,
    /// The script has a handler that runs on every frame.
    wants_frames: bool,
}

struct Vm {
    lua: Lua,
    /// See `tick` in the prelude.
    tick: Function,
    /// See `start` in the prelude.
    start: Function,
    /// Code of the script, it runs on the first update.
    main: Option<Function>,
    budget: Arc<Budget>,
    log_lines: Arc<AtomicU32>,
}

struct TickResult {
    scene: Value,
    audio: Value,
    next_wake_ms: f64,
    wants_frames: bool,
    errors: Vec<String>,
}

/// Values returned from `tick` and `start` in the prelude.
type TickValues = (Value, Value, f64, bool, Option<Vec<mlua::LuaString>>);

impl From<TickValues> for TickResult {
    fn from((scene, audio, next_wake_ms, wants_frames, errors): TickValues) -> Self {
        // An error message can be any sequence of bytes.
        let errors = errors.unwrap_or_default();
        let errors = errors.iter().map(|err| err.to_string_lossy());
        Self {
            scene,
            audio,
            next_wake_ms,
            wants_frames,
            errors: errors.collect(),
        }
    }
}

impl LuaScript {
    pub fn new(
        spec: &ScriptSpec,
        ctx: ScriptOutputCtx,
        options: LuaRuntimeOptions,
    ) -> Result<Self, ScriptError> {
        let mut vm = Vm::new(ctx, options)
            .map_err(|err| ScriptError::Load(format!("Failed to create Lua VM: {err}")))?;
        let main = vm
            .lua
            .load(&*spec.source)
            .set_name("=script")
            // Bytecode is not verified when it is loaded, it can't come from a script.
            .set_mode(ChunkMode::Text)
            .into_function()
            .map_err(|err| ScriptError::Load(error_message(&err)))?;
        vm.main = Some(main);

        Ok(Self {
            options,
            vm: Some(vm),
            pending_props: Some(spec.props.clone()),
            next_wake_ms: f64::INFINITY,
            wants_frames: false,
        })
    }
}

impl Script for LuaScript {
    fn update(&mut self, frame: &ScriptFrame<'_>) -> Result<ScriptUpdate, ScriptError> {
        let Some(vm) = &mut self.vm else {
            return Ok(ScriptUpdate::default());
        };
        let time_ms = frame.pts.as_secs_f64() * 1000.0;
        let is_start = vm.main.is_some();
        let is_idle = frame.events.is_empty()
            && self.pending_props.is_none()
            && !self.wants_frames
            && time_ms < self.next_wake_ms;
        if !is_start && is_idle {
            return Ok(ScriptUpdate::default());
        }

        let time_limit = match is_start {
            true => self.options.start_time_limit,
            false => self.options.update_time_limit,
        };
        vm.log_lines.store(0, Ordering::Relaxed);
        vm.budget.start(time_limit);
        let result = vm
            .run(frame, time_ms, self.pending_props.take())
            .and_then(|tick| {
                self.next_wake_ms = tick.next_wake_ms;
                self.wants_frames = tick.wants_frames;
                vm.convert_result(tick, is_start)
            });
        if vm.budget.finish() {
            self.vm = None;
            return Err(ScriptError::Terminated(format!(
                "Script used more than {time_limit:?} of CPU time for a single update."
            )));
        }
        result
    }

    fn set_props(&mut self, props: serde_json::Value) {
        self.pending_props = Some(props);
    }
}

impl Vm {
    fn new(ctx: ScriptOutputCtx, options: LuaRuntimeOptions) -> mlua::Result<Self> {
        // Libraries that do not give access to anything outside of the VM. `OS` in Luau is
        // limited to reading a clock.
        let libs = StdLib::COROUTINE
            | StdLib::TABLE
            | StdLib::OS
            | StdLib::STRING
            | StdLib::UTF8
            | StdLib::BIT
            | StdLib::MATH
            | StdLib::BUFFER;
        let lua = Lua::new_with(libs, LuaOptions::new())?;
        let budget = Arc::new(Budget::new());
        let log_lines = Arc::new(AtomicU32::new(0));

        let measure = {
            let host = ctx.host.clone();
            let budget = budget.clone();
            // Returns `(width, height, error)`. The error is raised on the Lua side, so it
            // points to the line that called `smelter.measure`.
            lua.create_function(move |lua, component: Value| {
                let component = match component_from_lua(lua, component) {
                    Ok(component) => component,
                    Err(err) => return Ok((None, None, Some(err))),
                };
                let size = match host.measure_component(component) {
                    Ok(size) => size,
                    Err(err) => return Ok((None, None, Some(ErrorStack::new(&err).into_string()))),
                };
                // The VM can't interrupt host code, e.g. layout of a very long text.
                if budget.check() {
                    return Ok((None, None, Some("time limit exceeded".to_string())));
                }
                Ok((size.width, size.height, None))
            })?
        };

        let log = {
            let output_id = ctx.output_id.clone();
            let log_lines = log_lines.clone();
            lua.create_function(move |_, msg: mlua::LuaString| {
                let line = log_lines.fetch_add(1, Ordering::Relaxed);
                if line < MAX_LOG_LINES_PER_UPDATE {
                    let bytes = msg.as_bytes();
                    let msg =
                        String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_LOG_LINE_BYTES)]);
                    info!(output_id = %output_id, "[script] {msg}");
                } else if line == MAX_LOG_LINES_PER_UPDATE {
                    warn!(output_id = %output_id, "Script prints too much, dropping messages.");
                }
                Ok(())
            })?
        };

        let output = lua.create_table()?;
        output.raw_set("id", &*ctx.output_id.0)?;
        output.raw_set("width", ctx.resolution.width)?;
        output.raw_set("height", ctx.resolution.height)?;
        output.raw_set(
            "framerate",
            ctx.framerate.num as f64 / ctx.framerate.den as f64,
        )?;

        let (tick, start): (Function, Function) = lua
            .load(PRELUDE)
            .set_name("=smelter")
            .set_mode(ChunkMode::Text)
            .call((measure, log, output))?;

        // From now on, globals and standard libraries are read-only. Globals defined by the
        // script are stored in a separate table.
        lua.sandbox(true)?;
        lua.set_memory_limit(options.memory_limit)?;
        {
            let budget = budget.clone();
            lua.set_interrupt(move |_| match budget.check_on_interrupt() {
                true => Err(mlua::Error::runtime("time limit exceeded")),
                false => Ok(VmState::Continue),
            });
        }

        Ok(Self {
            lua,
            tick,
            start,
            main: None,
            budget,
            log_lines,
        })
    }

    fn run(
        &mut self,
        frame: &ScriptFrame<'_>,
        time_ms: f64,
        props: Option<serde_json::Value>,
    ) -> Result<TickResult, ScriptError> {
        let runtime_error = |err: mlua::Error| ScriptError::Runtime(error_message(&err));

        if let Some(main) = self.main.take() {
            // Makes inputs and props available for the code of the script. Nothing is rendered,
            // because render function is not known yet.
            self.tick(time_ms, Some(frame.inputs), &[], props)
                .map_err(runtime_error)?;
            let result: Value = main.call(()).map_err(runtime_error)?;
            return self
                .start
                .call::<TickValues>(result)
                .map(TickResult::from)
                .map_err(runtime_error);
        }

        let inputs = match frame.events.is_empty() {
            true => None,
            false => Some(frame.inputs),
        };
        let events: Vec<_> = frame
            .events
            .iter()
            .map(|event| (event_name(event.kind), &*event.input_id.0))
            .collect();
        self.tick(time_ms, inputs, &events, props)
            .map_err(runtime_error)
    }

    fn tick(
        &self,
        time_ms: f64,
        inputs: Option<&[ScriptInput]>,
        events: &[(&str, &str)],
        props: Option<serde_json::Value>,
    ) -> mlua::Result<TickResult> {
        let (inputs, events) = match inputs {
            Some(inputs) => {
                let events_list = self.lua.create_table_with_capacity(events.len() * 2, 0)?;
                for (name, input_id) in events {
                    events_list.raw_push(*name)?;
                    events_list.raw_push(*input_id)?;
                }
                (
                    Value::Table(self.inputs_to_lua(inputs)?),
                    Value::Table(events_list),
                )
            }
            None => (Value::Nil, Value::Nil),
        };
        let props_changed = props.is_some();
        let props = match props {
            Some(props) => json_to_lua(&self.lua, &props)?,
            None => Value::Nil,
        };

        let values: TickValues = self
            .tick
            .call((time_ms, inputs, events, props_changed, props))?;
        Ok(values.into())
    }

    fn inputs_to_lua(&self, inputs: &[ScriptInput]) -> mlua::Result<Table> {
        let list = self.lua.create_table_with_capacity(inputs.len(), 0)?;
        for input in inputs {
            let table = self.lua.create_table_with_capacity(0, 5)?;
            table.raw_set("id", &*input.input_id.0)?;
            table.raw_set("video", input.video.map(track_state_name))?;
            table.raw_set("audio", input.audio.map(track_state_name))?;
            if let Some(resolution) = input.resolution {
                table.raw_set("width", resolution.width)?;
                table.raw_set("height", resolution.height)?;
            }
            list.raw_push(table)?;
        }
        Ok(list)
    }

    /// - `is_start` - errors are returned instead of being reported along with the result,
    ///   so a script that does not work from the start is rejected.
    fn convert_result(
        &self,
        tick: TickResult,
        is_start: bool,
    ) -> Result<ScriptUpdate, ScriptError> {
        let mut update = ScriptUpdate {
            errors: tick.errors.into_iter().map(ScriptError::Runtime).collect(),
            ..Default::default()
        };
        if !tick.scene.is_nil() {
            match component_from_lua(&self.lua, tick.scene) {
                Ok(component) => update.video = Some(component),
                Err(err) => update.errors.push(ScriptError::InvalidScene(err)),
            }
            if !tick.audio.is_nil() {
                match audio_from_lua(&self.lua, tick.audio) {
                    Ok(audio) => update.audio = Some(audio),
                    Err(err) => update.errors.push(ScriptError::InvalidScene(err)),
                }
            }
        }

        if is_start && !update.errors.is_empty() {
            return Err(update.errors.remove(0));
        }
        Ok(update)
    }
}

fn track_state_name(state: TrackState) -> &'static str {
    match state {
        TrackState::Ready => "ready",
        TrackState::Playing => "playing",
        TrackState::Paused => "paused",
        TrackState::Finished => "finished",
    }
}

fn event_name(kind: ScriptEventKind) -> &'static str {
    match kind {
        ScriptEventKind::InputRegistered => "input_registered",
        ScriptEventKind::InputUnregistered => "input_unregistered",
        ScriptEventKind::VideoDelivered => "video_input_delivered",
        ScriptEventKind::VideoPlaying => "video_input_playing",
        ScriptEventKind::VideoPaused => "video_input_paused",
        ScriptEventKind::VideoEos => "video_input_eos",
        ScriptEventKind::VideoResized => "video_input_resized",
        ScriptEventKind::AudioDelivered => "audio_input_delivered",
        ScriptEventKind::AudioPlaying => "audio_input_playing",
        ScriptEventKind::AudioPaused => "audio_input_paused",
        ScriptEventKind::AudioEos => "audio_input_eos",
    }
}
