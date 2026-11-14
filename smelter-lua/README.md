# Scene scripts

A scene of an output can be defined by a Lua script instead of a component tree. The script runs
inside Smelter, on the renderer thread, right before a frame is rendered. Its reactions to input
changes and its timers are frame accurate, there is no round trip to the application that controls
Smelter.

```http
POST /api/output/output_1/register
```

```json
{
  "type": "mp4",
  "path": "/tmp/output.mp4",
  "video": {
    "resolution": { "width": 1280, "height": 720 },
    "encoder": { "type": "ffmpeg_h264" },
    "initial": {
      "script": {
        "source": "return function() ... end",
        "props": { "title": "Hello" }
      }
    }
  }
}
```

`POST /api/output/:id/update` accepts the same `script` object in the `video` field:

- If `source` is exactly the same as the source of the running script, the script is not restarted.
  It keeps its state and only `props` are replaced.
- Otherwise, the new script replaces the old one.
- An update with `root` stops the script.

A request that starts a script fails if the script can't be compiled, its top-level code fails, or
it does not produce a valid scene.

## Script

Scripts are written in [Luau](https://luau.org), a dialect of Lua 5.1 with a few additions (e.g.
`+=`, `continue`, string interpolation, optional type annotations).

A script returns a render function. The function returns a component tree, built from tables that
have the same fields as components in the HTTP API.

```lua
return function()
    local tiles = {}
    for _, input in smelter.inputs() do
        if input.video == "playing" then
            table.insert(tiles, InputStream { input_id = input.id })
        end
    end
    return Tiles { id = "grid", transition = { duration_ms = 300 }, children = tiles }
end
```

The render function is called when the script starts and then every time something could change
the result:

- state of any input changed,
- props were updated,
- a timer or a `frame` handler was called.

The scene is updated only if the returned tree is different from the previous one. A script that
does not need any of that can return a component instead of a function.

Code outside of the render function runs once. Use it to keep state, register event handlers and
start timers.

### Components

`View`, `Tiles`, `Rescaler`, `Shader`, `WebView`, `Text`, `Image` and `InputStream` create a
component. They take a table with the same fields as the component in the HTTP API and set its
`type`.

```lua
View {
    direction = "column",
    background_color = "#1B1B2FFF",
    Text { "Title", font_size = 40 },                  -- the same as `text = "Title"`
    Rescaler { InputStream { input_id = "input_1" } }, -- the same as `child = InputStream { ... }`
    show_footer and footer(),                          -- `false` and `nil` are skipped
    list_of_components,                                -- lists are flattened
}
```

Positional values are children. They can be mixed with `children = { ... }`, which is added first.

### Audio

The render function can return audio configuration as a second value. It has the same fields as
`audio` in the update request. If the script never returns it, audio is controlled only with the
HTTP API. After the script returned it for the first time, `audio` from requests is ignored for as
long as that script runs.

```lua
return function()
    return View {}, { inputs = { { input_id = "input_1", volume = 0.5 } } }
end
```

## API

### `smelter.inputs()`, `smelter.input(id)`

List of all registered inputs in registration order, or a single input (`nil` if it does not
exist). An input is a read-only table:

| field             | description                                                             |
| ----------------- | ----------------------------------------------------------------------- |
| `id`              | Input ID.                                                               |
| `video`, `audio`  | `"ready"`, `"playing"`, `"paused"`, `"finished"`, or `nil` before any data was delivered. |
| `width`, `height` | Resolution of the last video frame, `nil` before the first frame.       |

### `smelter.on(event, handler)`

Calls `handler` when the event happens. Returns a subscription with a `cancel` method.

| event                                          | handler arguments  |                                          |
| ---------------------------------------------- | ------------------ | ---------------------------------------- |
| `input_registered`, `input_unregistered`       | `input`            |                                          |
| `video_input_delivered`, `audio_input_delivered` | `input`          | First data was received.                 |
| `video_input_playing`, `audio_input_playing`   | `input`            |                                          |
| `video_input_paused`, `audio_input_paused`     | `input`            |                                          |
| `video_input_eos`, `audio_input_eos`           | `input`            |                                          |
| `video_input_resized`                          | `input`            | Resolution of the input changed.         |
| `props`                                        | `props`, `previous` | Props were updated.                     |
| `frame`                                        | `time`             | Called before every frame.               |

`video_input_playing`, `video_input_eos` and `video_input_resized` are in sync with frames: the
handler runs before the first frame the change applies to. If the scene shows an input while it
is `"playing"`, then the first and the last frame of that input are rendered, and no frame is
rendered without it. Other input events, and `video_input_playing` of an input that was paused,
are delivered on the first frame after they happen.

A `frame` handler makes the script run on every frame. Prefer `transition` on components for
animations, and timers for anything that does not change on every frame.

### Timers

- `smelter.after(ms, callback)` - calls `callback` once, `ms` milliseconds from now.
- `smelter.every(ms, callback)` - calls `callback` every `ms` milliseconds.
- `smelter.at(ms, callback)` - calls `callback` once at `ms` milliseconds since Smelter was
  started.

All of them return a timer with a `cancel` method.

Timers use timestamps of rendered frames, not a wall clock. A callback runs before the first frame
with a timestamp that is not lower than the time of the timer, so results are the same for live
and offline processing. If more than one interval of `smelter.every` passed between two frames,
the callback is called once.

### `smelter.time()`

Timestamp of the frame that is being prepared, in milliseconds since Smelter was started.

### `smelter.measure(component)`

Size of a component: `{ width = number?, height = number? }`.

- `Text` - size of the text after layout.
- `Image` - size of the registered image, or the size after applying `width`/`height`.
- `InputStream` - resolution of the last frame.
- `Shader`, `WebView` - their resolution.
- `View`, `Rescaler`, `Tiles` - size with padding and border if `width`/`height` are defined.
  `nil` for a dimension that depends on the parent.

A `View` without `width`/`height` takes all the space its parent has left, so a box that should be
as big as its content needs to be sized based on the measured content.

```lua
local label = Text { text = "Hello", font_size = 32 }
local size = smelter.measure(label)
local box = View { width = size.width + 16, height = size.height + 16, padding = 8, label }
```

### `smelter.output`

`{ id = string, width = number, height = number, framerate = number }`

### `smelter.props()`

Read-only value from the `props` field of the request.

### `print(...)`

Writes to the Smelter log.

## Errors

- An error in a handler, a timer callback or the render function is reported as an `OUTPUT_ERROR`
  event and in the log. The script keeps running and the output keeps the last valid scene.
- Only the first few errors in every 5 seconds are reported.
- A script that exceeds the time limit is terminated. The output keeps the last scene. An update
  request can start the script again.

## Sandbox and limits

Every script runs in a separate VM.

- Only safe parts of the standard library are available: `math`, `string`, `table`, `utf8`,
  `bit32`, `buffer`, `coroutine` and `os.clock`/`os.date`/`os.time`/`os.difftime`. There is no
  access to files, network, processes or environment variables, and no way to load more code.
- Globals and the standard library are read-only.
- Memory is limited.
- CPU time is limited. The limit is checked in loops, function calls and pattern matching, so it
  also stops code that never returns, like an infinite loop or a pattern that takes forever to
  match. It is not checked inside other library functions. Most of them need time proportional
  to the size of their arguments, which is limited by the memory limit (a few milliseconds for
  the largest possible string). Functions that need more reject large arguments: `string.find`
  and `string.split` with long texts, `table.sort` without a comparator for lists longer than
  100000.
- A scene can have up to 20000 values (fields and list elements), 64 levels of nested components
  and 256 KB of text. A table used in a few places counts every time. Numbers have to be finite.

The sandbox limits what the code of a script can do. The scene it produces is not restricted in
any other way than a scene sent with the HTTP API, e.g. it can use any registered image, shader
or input.

Starting a script (a register or update request with a new `source`) blocks rendering until the
top-level code of the script and the first render are done, so `SMELTER_SCRIPT_START_TIME_LIMIT_MS`
is also the longest pause such a request can cause.

| environment variable                  | default | description                                         |
| ------------------------------------- | ------- | --------------------------------------------------- |
| `SMELTER_SCRIPT_ENABLE`               | `true`  | Enables scene scripts.                              |
| `SMELTER_SCRIPT_MEMORY_LIMIT_MB`      | `16`    | Memory limit of a single script.                    |
| `SMELTER_SCRIPT_START_TIME_LIMIT_MS`  | `100`   | CPU time limit for starting a script.               |
| `SMELTER_SCRIPT_UPDATE_TIME_LIMIT_MS` | `10`    | CPU time limit for everything a script does before a single frame. |

## Editor support

`smelter.d.luau` has type definitions for [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp).
Add it to the definition files of the language server, or check a script from the command line:

```
luau-lsp analyze --definitions=smelter.d.luau scene.luau
```
