-- Lua side of the scene script API.
--
-- It runs before the sandbox is enabled and before any code of the script. Nothing here gives
-- access to more than a script is allowed to do: host functions are safe to call with any
-- arguments, and the rest is regular Lua code that only keeps the script's own state.

local host_measure, host_log, output = ...

local error, ipairs, pcall, select, tostring, type = error, ipairs, pcall, select, tostring, type
local math_floor, math_huge = math.floor, math.huge
local table_clone, table_concat, table_create = table.clone, table.concat, table.create
local table_freeze, table_isfrozen, table_sort = table.freeze, table.isfrozen, table.sort
local table_maxn = table.maxn
local string_find, string_split, string_sub = string.find, string.split, string.sub

---------------------------------------------------------------------------------------------------
-- Components
--
-- A component is a table that has the same fields as a component in the HTTP API. Constructors
-- only add `type` and move positional values to `children`/`child`/`text`.
---------------------------------------------------------------------------------------------------

local function check_props(name, props)
	if type(props) ~= "table" then
		error(`{name} expects a table with properties, got {type(props)}`, 3)
	end
end

-- `value` is a component, a list of components (lists can be nested) or `false`/`nil`, which
-- is skipped to support `condition and Component { ... }`.
--
-- Returns type of a value that can't be a child.
local function collect_children(children, value)
	if not value then
		return nil
	end
	if type(value) ~= "table" then
		return type(value)
	end
	if value.type ~= nil then
		children[#children + 1] = value
		return nil
	end
	for _, item in ipairs(value) do
		local invalid = collect_children(children, item)
		if invalid ~= nil then
			return invalid
		end
	end
	return nil
end

local function container(name, kind)
	return function(props)
		check_props(name, props)
		-- Not `#props`, a skipped child can be `nil`, so the list can have holes.
		local count = table_maxn(props)
		local explicit = props.children
		if count > 0 or explicit ~= nil then
			local children = {}
			local invalid = collect_children(children, explicit)
			for index = 1, count do
				invalid = invalid or collect_children(children, props[index])
				props[index] = nil
			end
			if invalid ~= nil then
				error(`{name}: a child has to be a component or a list of components, got {invalid}`, 2)
			end
			props.children = children
		end
		props.type = kind
		return props
	end
end

local function leaf(name, kind)
	return function(props)
		check_props(name, props)
		props.type = kind
		return props
	end
end

local function Text(props)
	check_props("Text", props)
	local text = props.text
	if text == nil then
		text = props[1]
		props[1] = nil
	end
	if type(text) == "number" then
		text = tostring(text)
	end
	props.text = text
	props.type = "text"
	return props
end

local function Rescaler(props)
	check_props("Rescaler", props)
	if #props > 1 then
		error("Rescaler accepts only one child", 2)
	end
	if props.child == nil then
		props.child = props[1]
	end
	props[1] = nil
	props.type = "rescaler"
	return props
end

_G.View = container("View", "view")
_G.Tiles = container("Tiles", "tiles")
_G.Shader = container("Shader", "shader")
_G.WebView = container("WebView", "web_view")
_G.Rescaler = Rescaler
_G.Text = Text
_G.Image = leaf("Image", "image")
_G.InputStream = leaf("InputStream", "input_stream")

---------------------------------------------------------------------------------------------------
-- State
---------------------------------------------------------------------------------------------------

local EVENTS = table_freeze({
	input_registered = true,
	input_unregistered = true,
	video_input_delivered = true,
	video_input_playing = true,
	video_input_paused = true,
	video_input_eos = true,
	video_input_resized = true,
	audio_input_delivered = true,
	audio_input_playing = true,
	audio_input_paused = true,
	audio_input_eos = true,
	props = true,
	frame = true,
})

local now = 0
local inputs = table_freeze({})
local inputs_by_id = {}
local props = nil

-- [event name] = list of { fn = function, active = boolean }
local listeners = {}
local frame_listeners = 0
-- Time of the last tick, "frame" event is emitted once per frame.
local last_frame_time = -math_huge

-- list of { due = number, seq = number, fn = function, interval = number?, active = boolean }
local timers = {}
local timer_seq = 0

-- Render function or a component.
local render = nil
local dirty = true
-- Errors of handlers called during the current tick.
local errors = nil

-- Errors are logged and sent to clients, so their number and size is limited.
local MAX_ERRORS_PER_TICK = 8
local MAX_ERROR_LENGTH = 1000

local function report(err)
	errors = errors or {}
	if #errors >= MAX_ERRORS_PER_TICK then
		return
	end
	-- An error can be any value, `tostring` can call code of the script.
	local ok, message = pcall(tostring, err)
	if not ok or type(message) ~= "string" then
		message = "error value can't be converted to a string"
	elseif #message > MAX_ERROR_LENGTH then
		message = string_sub(message, 1, MAX_ERROR_LENGTH) .. "..."
	end
	errors[#errors + 1] = message
end

local function call(fn, a, b)
	local ok, err = pcall(fn, a, b)
	if not ok then
		report(err)
	end
end

local function emit(event, a, b)
	local list = listeners[event]
	if list == nil then
		return
	end
	-- Handlers can subscribe and cancel subscriptions.
	for _, listener in ipairs(table_clone(list)) do
		if listener.active then
			call(listener.fn, a, b)
		end
	end
end

local function new_handle(cancel)
	return table_freeze({ cancel = function()
		cancel()
	end })
end

local function deep_freeze(value)
	if type(value) == "table" and not table_isfrozen(value) then
		for _, item in value do
			deep_freeze(item)
		end
		table_freeze(value)
	end
	return value
end

---------------------------------------------------------------------------------------------------
-- Public API
---------------------------------------------------------------------------------------------------

local smelter = {}

smelter.output = table_freeze(output)

-- PTS of the frame that is being prepared, in milliseconds since the pipeline start.
function smelter.time()
	return now
end

function smelter.props()
	return props
end

function smelter.inputs()
	return inputs
end

function smelter.input(id)
	return inputs_by_id[id]
end

function smelter.measure(component)
	if type(component) ~= "table" then
		error(`smelter.measure expects a component, got {type(component)}`, 2)
	end
	local width, height, err = host_measure(component)
	if err ~= nil then
		error(`smelter.measure failed: {err}`, 2)
	end
	return { width = width, height = height }
end

function smelter.on(event, fn)
	if EVENTS[event] == nil then
		error(`unknown event "{tostring(event)}"`, 2)
	end
	if type(fn) ~= "function" then
		error(`event handler has to be a function, got {type(fn)}`, 2)
	end
	local list = listeners[event]
	if list == nil then
		list = {}
		listeners[event] = list
	end
	local listener = { fn = fn, active = true }
	list[#list + 1] = listener
	if event == "frame" then
		frame_listeners += 1
	end

	return new_handle(function()
		if not listener.active then
			return
		end
		listener.active = false
		for index, item in ipairs(list) do
			if item == listener then
				table.remove(list, index)
				break
			end
		end
		if event == "frame" then
			frame_listeners -= 1
		end
	end)
end

local function add_timer(name, due, interval, fn)
	if type(due) ~= "number" or due ~= due or due == math_huge or due == -math_huge then
		error(`{name} expects time in milliseconds, got {tostring(due)}`, 3)
	end
	if type(fn) ~= "function" then
		error(`{name} expects a callback function, got {type(fn)}`, 3)
	end
	timer_seq += 1
	local timer = { due = due, seq = timer_seq, fn = fn, interval = interval, active = true }
	timers[#timers + 1] = timer
	return new_handle(function()
		timer.active = false
	end)
end

local function check_delay(name, ms)
	if type(ms) ~= "number" or not (ms >= 0) then
		error(`{name} expects a non-negative number of milliseconds, got {tostring(ms)}`, 3)
	end
end

function smelter.after(ms, fn)
	check_delay("smelter.after", ms)
	return add_timer("smelter.after", now + ms, nil, fn)
end

function smelter.every(ms, fn)
	check_delay("smelter.every", ms)
	if ms == 0 then
		error("smelter.every expects an interval greater than zero", 2)
	end
	return add_timer("smelter.every", now + ms, ms, fn)
end

function smelter.at(ms, fn)
	return add_timer("smelter.at", ms, nil, fn)
end

_G.smelter = table_freeze(smelter)

_G.print = function(...)
	local count = select("#", ...)
	local parts = table_create(count)
	for index = 1, count do
		parts[index] = tostring((select(index, ...)))
	end
	host_log(table_concat(parts, "\t"))
end

-- Functions that load code or change environment of other functions.
_G.require = nil
_G.loadstring = nil
_G.getfenv = nil
_G.setfenv = nil

-- The time limit is checked when the VM runs code of the script and when it matches patterns,
-- but not inside other library functions. Most of them need time proportional to the size of
-- their arguments, which is limited by the memory limit. Functions below need more, so they
-- reject arguments that are too large.

-- Sorting without a comparator does not run any code of the script.
local MAX_SORT_WITHOUT_COMPARATOR = 100_000

table.sort = function(list, comparator)
	if comparator == nil and type(list) == "table" and #list > MAX_SORT_WITHOUT_COMPARATOR then
		error("table.sort: list is too long to sort without a comparator", 2)
	end
	return table_sort(list, comparator)
end

-- Search for a plain string can take time proportional to `#text * #searched`.
local MAX_PLAIN_SEARCH_WORK = 50_000_000

local function check_plain_search(name, text, searched)
	if type(text) == "string" and type(searched) == "string" and #text * #searched > MAX_PLAIN_SEARCH_WORK then
		error(`{name}: arguments are too long`, 3)
	end
end

string.find = function(text, pattern, ...)
	check_plain_search("string.find", text, pattern)
	return string_find(text, pattern, ...)
end

string.split = function(text, separator)
	check_plain_search("string.split", text, separator)
	return string_split(text, separator)
end

---------------------------------------------------------------------------------------------------
-- Functions called by the host
---------------------------------------------------------------------------------------------------

-- Timers added by callbacks do not run before the next tick, so a timer can't loop forever.
local function run_timers()
	local due = nil
	for _, timer in ipairs(timers) do
		if timer.active and timer.due <= now then
			due = due or {}
			due[#due + 1] = timer
		end
	end
	if due == nil then
		return false
	end

	table_sort(due, function(a, b)
		if a.due ~= b.due then
			return a.due < b.due
		end
		return a.seq < b.seq
	end)
	for _, timer in ipairs(due) do
		if timer.active then
			local interval = timer.interval
			if interval == nil then
				timer.active = false
			else
				-- Intervals that already passed are skipped.
				timer.due += interval * (math_floor((now - timer.due) / interval) + 1)
			end
			call(timer.fn)
		end
	end
	return true
end

-- Removes cancelled timers, returns time of the next one.
local function next_timer()
	local next_due = math_huge
	local count = 0
	for index = 1, #timers do
		local timer = timers[index]
		if timer.active then
			count += 1
			timers[count] = timer
			if timer.due < next_due then
				next_due = timer.due
			end
		end
	end
	for index = count + 1, #timers do
		timers[index] = nil
	end
	return next_due
end

local function run_render()
	if type(render) ~= "function" then
		return render, nil
	end
	local ok, scene, audio = pcall(render)
	if not ok then
		report(scene)
		return nil, nil
	end
	if type(scene) ~= "table" then
		report(`render function has to return a component, got {type(scene)}`)
		return nil, nil
	end
	return scene, audio
end

-- Runs everything that is due for the frame at `time` and renders the scene if anything could
-- change it.
--
-- - `new_inputs` - list of all inputs, present only if `events` is not empty.
-- - `events` - flat list of (event name, input id) pairs.
--
-- Returns: scene (or nil if it was not rendered), audio config, time of the next timer, whether
-- tick needs to be called for every frame, list of errors.
local function tick(time, new_inputs, events, props_changed, new_props)
	now = time
	errors = nil

	if new_inputs ~= nil then
		local previous_by_id = inputs_by_id
		local by_id = {}
		for _, input in ipairs(new_inputs) do
			by_id[input.id] = table_freeze(input)
		end
		inputs = table_freeze(new_inputs)
		inputs_by_id = by_id
		dirty = true

		for index = 1, #events, 2 do
			local event, id = events[index], events[index + 1]
			local input = if event == "input_unregistered" then previous_by_id[id] else by_id[id]
			emit(event, input)
		end
	end

	if props_changed then
		local previous = props
		props = deep_freeze(new_props)
		dirty = true
		emit("props", props, previous)
	end

	if run_timers() then
		dirty = true
	end

	-- Tick can be called more than once for the same frame, e.g. when props are updated.
	if frame_listeners > 0 and now > last_frame_time then
		dirty = true
		emit("frame", now)
	end
	last_frame_time = now

	local scene, audio = nil, nil
	if dirty and render ~= nil then
		dirty = false
		scene, audio = run_render()
	end
	return scene, audio, next_timer(), frame_listeners > 0, errors
end

-- Renders the first scene. Timers and frame handlers do not run before the first tick.
--
-- - `result` - the value returned by the script.
--
-- Returns the same values as `tick`.
local function start(result)
	local kind = type(result)
	if kind ~= "function" and kind ~= "table" then
		error(`script has to return a render function or a component, got {kind}`, 0)
	end
	render = result
	dirty = false
	errors = nil
	-- The first frame can have the same time as the start.
	last_frame_time = -math_huge
	local scene, audio = run_render()
	return scene, audio, next_timer(), frame_listeners > 0, errors
end

return tick, start
