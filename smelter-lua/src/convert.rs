use mlua::{
    Lua, LuaSerdeExt, Value,
    serde::{DeserializeOptions, SerializeOptions},
};
use smelter_api as api;
use smelter_core::AudioMixerConfig;
use smelter_render::scene;

/// Limit of nested components in a scene. Scene is processed recursively, so the limit protects
/// the stack.
pub(crate) const MAX_SCENE_DEPTH: usize = 64;

/// Limit of nested tables. Each nested component adds 2 tables: the component and the list of
/// its children. The rest is for values of component fields.
const MAX_TABLE_DEPTH: usize = 2 * MAX_SCENE_DEPTH + 8;

/// Limit of values (fields and list elements) in a single scene.
///
/// Tables are converted by code that the VM can't interrupt, so the time limit does not apply
/// there. A table that is used in a few places is counted every time, otherwise a small number
/// of tables that refer to each other could describe a scene of any size.
pub(crate) const MAX_SCENE_VALUES: usize = 20_000;

/// Limit of total length of strings in a single scene. Layout of a text is not limited by the
/// time limit either.
pub(crate) const MAX_SCENE_STRING_BYTES: usize = 256 * 1024;

const DESERIALIZE_OPTIONS: DeserializeOptions = DeserializeOptions::new()
    // An empty Lua table does not tell if it is a list or a map. Lists are more common in
    // a scene, e.g. `children = {}`.
    .encode_empty_tables_as_array(true)
    .recursion_limit(MAX_TABLE_DEPTH);

/// JSON `null` becomes `nil`, lists and maps become plain tables.
const SERIALIZE_OPTIONS: SerializeOptions = SerializeOptions::new()
    .serialize_none_to_null(false)
    .serialize_unit_to_null(false)
    .set_array_metatable(false);

/// Converts a table with the same structure as a component in the HTTP API.
pub(crate) fn component_from_lua(lua: &Lua, value: Value) -> Result<scene::Component, String> {
    SizeLimits::check(&value)?;

    let component: api::Component = lua
        .from_value_with(value.clone(), DESERIALIZE_OPTIONS)
        .map_err(|err| {
            let mut path = "root".to_string();
            let msg = find_invalid_component(lua, &value, &mut path, 0)
                .unwrap_or_else(|| error_message(&err));
            format!("Invalid component at `{path}`: {msg}")
        })?;
    if component_depth(&component) > MAX_SCENE_DEPTH {
        return Err(too_deep_error());
    }
    component
        .try_into()
        .map_err(|err: api::TypeError| err.to_string())
}

fn too_deep_error() -> String {
    format!("Scene has more than {MAX_SCENE_DEPTH} levels of nested components.")
}

/// Checks that a value returned by a script is small enough to be converted.
struct SizeLimits {
    values: usize,
    string_bytes: usize,
}

impl SizeLimits {
    fn check(value: &Value) -> Result<(), String> {
        let mut limits = Self {
            values: MAX_SCENE_VALUES,
            string_bytes: MAX_SCENE_STRING_BYTES,
        };
        limits.visit(value, 0)
    }

    fn visit(&mut self, value: &Value, depth: usize) -> Result<(), String> {
        match value {
            Value::Table(table) => {
                // Also stops on tables that contain themselves.
                if depth >= MAX_TABLE_DEPTH {
                    return Err(too_deep_error());
                }
                // Metamethods are not called, the same as during the conversion.
                for pair in table.pairs::<Value, Value>() {
                    let (_key, value) = pair.map_err(|err| error_message(&err))?;
                    self.values = self
                        .values
                        .checked_sub(1)
                        .ok_or_else(|| format!("Scene has more than {MAX_SCENE_VALUES} values."))?;
                    self.visit(&value, depth + 1)?;
                }
                Ok(())
            }
            Value::String(string) => {
                let len = string.as_bytes().len();
                self.string_bytes = self.string_bytes.checked_sub(len).ok_or_else(|| {
                    format!("Scene has more than {MAX_SCENE_STRING_BYTES} bytes of text.")
                })?;
                Ok(())
            }
            // JSON can't represent them, so the rest of the code does not expect them.
            Value::Number(number) if !number.is_finite() => Err(format!(
                "Scene contains a number that is not finite ({number})."
            )),
            _ => Ok(()),
        }
    }
}

fn component_depth(component: &api::Component) -> usize {
    let children = match component {
        api::Component::View(view) => view.children.as_deref(),
        api::Component::Tiles(tiles) => tiles.children.as_deref(),
        api::Component::Shader(shader) => shader.children.as_deref(),
        api::Component::WebView(web_view) => web_view.children.as_deref(),
        api::Component::Rescaler(rescaler) => Some(std::slice::from_ref(&*rescaler.child)),
        api::Component::InputStream(_) | api::Component::Image(_) | api::Component::Text(_) => None,
    };
    let children_depth = children
        .unwrap_or_default()
        .iter()
        .map(component_depth)
        .max();
    1 + children_depth.unwrap_or(0)
}

/// Converts a table with the same structure as audio config in the HTTP API.
pub(crate) fn audio_from_lua(lua: &Lua, value: Value) -> Result<AudioMixerConfig, String> {
    SizeLimits::check(&value)?;
    let audio: api::AudioScene = lua
        .from_value_with(value, DESERIALIZE_OPTIONS)
        .map_err(|err| format!("Invalid audio config: {}", error_message(&err)))?;
    audio
        .try_into()
        .map_err(|err: api::TypeError| err.to_string())
}

pub(crate) fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> mlua::Result<Value> {
    lua.to_value_with(value, SERIALIZE_OPTIONS)
}

/// Deserialization error does not tell where in the tree it happened. This function finds the
/// deepest component that fails on its own, so its children are valid. Returns the error of
/// that component and appends its location to `path`.
///
/// The search stops at `MAX_SEARCH_DEPTH`, a scene can be recursive.
fn find_invalid_component(
    lua: &Lua,
    value: &Value,
    path: &mut String,
    depth: usize,
) -> Option<String> {
    const MAX_SEARCH_DEPTH: usize = 16;

    let Value::Table(table) = value else {
        return Some(format!("expected a component, got {}", value.type_name()));
    };
    let err = lua
        .from_value_with::<api::Component>(value.clone(), DESERIALIZE_OPTIONS)
        .err()?;
    if depth >= MAX_SEARCH_DEPTH {
        return Some(error_message(&err));
    }

    if let Ok(Value::Table(children)) = table.raw_get::<Value>("children") {
        for (index, child) in children.sequence_values::<Value>().flatten().enumerate() {
            let len = path.len();
            // Lua lists start from 1
            path.push_str(&format!(".children[{}]", index + 1));
            if let Some(msg) = find_invalid_component(lua, &child, path, depth + 1) {
                return Some(msg);
            }
            path.truncate(len);
        }
    }
    if let Ok(child @ Value::Table(_)) = table.raw_get::<Value>("child") {
        let len = path.len();
        path.push_str(".child");
        if let Some(msg) = find_invalid_component(lua, &child, path, depth + 1) {
            return Some(msg);
        }
        path.truncate(len);
    }
    Some(error_message(&err))
}

/// Error message without a prefix that names the error variant.
pub(crate) fn error_message(err: &mlua::Error) -> String {
    match err {
        mlua::Error::DeserializeError(msg)
        | mlua::Error::SerializeError(msg)
        | mlua::Error::RuntimeError(msg)
        | mlua::Error::MemoryError(msg) => msg.clone(),
        mlua::Error::SyntaxError { message, .. } => message.clone(),
        mlua::Error::CallbackError { cause, .. } => error_message(cause),
        mlua::Error::WithContext { context, cause } => {
            format!("{context}: {}", error_message(cause))
        }
        err => err.to_string(),
    }
}
