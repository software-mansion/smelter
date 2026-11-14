-- Type definitions of the scene script API for luau-lsp:
--
--   luau-lsp analyze --definitions=smelter.d.luau scene.luau
--
-- Component properties are the same as fields of components in the HTTP API, they are not
-- type checked here.

export type Component = { type: string, [string]: any }

-- Components that always have a size.
export type SizedComponent = { type: "text" | "image" | "input_stream" | "shader" | "web_view", [string]: any }

export type Props = { [string | number]: any }

export type TrackState = "ready" | "playing" | "paused" | "finished"

-- Read-only.
export type Input = {
	id: string,
	video: TrackState?,
	audio: TrackState?,
	width: number?,
	height: number?,
}

export type Size = { width: number?, height: number? }

export type Handle = { cancel: (self: Handle) -> () }

export type InputEvent =
	"input_registered"
	| "input_unregistered"
	| "video_input_delivered"
	| "video_input_playing"
	| "video_input_paused"
	| "video_input_eos"
	| "video_input_resized"
	| "audio_input_delivered"
	| "audio_input_playing"
	| "audio_input_paused"
	| "audio_input_eos"

export type AudioConfig = { inputs: { { input_id: string, volume: number? } } }

declare function View(props: Props): Component
declare function Tiles(props: Props): Component
declare function Rescaler(props: Props): Component
declare function Shader(props: Props): SizedComponent
declare function WebView(props: Props): SizedComponent
declare function Text(props: Props): SizedComponent
declare function Image(props: Props): SizedComponent
declare function InputStream(props: Props): SizedComponent

declare smelter: {
	-- Read-only.
	output: { id: string, width: number, height: number, framerate: number },

	-- Timestamp of the frame that is being prepared, in milliseconds since the start.
	time: () -> number,
	props: () -> any,
	inputs: () -> { Input },
	input: (id: string) -> Input?,
	-- Size of a layout (`View`, `Rescaler`, `Tiles`) is known only if it is defined explicitly.
	measure: ((component: SizedComponent) -> { width: number, height: number })
		& ((component: Component) -> Size),

	on: ((event: InputEvent, handler: (input: Input) -> ()) -> Handle)
		& ((event: "props", handler: (props: any, previous: any) -> ()) -> Handle)
		& ((event: "frame", handler: (time: number) -> ()) -> Handle),

	after: (ms: number, callback: () -> ()) -> Handle,
	every: (ms: number, callback: () -> ()) -> Handle,
	at: (ms: number, callback: () -> ()) -> Handle,
}
