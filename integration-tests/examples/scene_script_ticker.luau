-- Bar at the bottom of the screen with texts that move from right to left in a loop, with
-- a separator between them.
--
-- Texts come from props and nothing is assumed about them: a text can be a single character
-- or much longer than the screen.
--
-- The script does not run on every frame. Every segment of the bar is a separate component
-- that moves across the screen with a single transition, so the script only runs when
-- a segment shows up.

local SPEED = 150 -- pixels per second
local BAR_HEIGHT = 64
local FONT_SIZE = 32
local SEPARATOR_WIDTH = 80
local SEPARATOR_SIZE = 10
-- Longer texts are not measured, they are split first.
local MAX_MEASURED_CHARACTERS = 100

local output = smelter.output

-- Width of a space between words of a text that was split.
local SPACE_WIDTH = smelter.measure(Text { text = "a a", font_size = FONT_SIZE }).width
	- smelter.measure(Text { text = "aa", font_size = FONT_SIZE }).width

---------------------------------------------------------------------------------------------------
-- Segments
--
-- A segment is a part of the bar that is a single component: a text, a part of a long text or
-- a separator. `width` is the distance between the start of this segment and the next one.
---------------------------------------------------------------------------------------------------

local function separator()
	return {
		width = SEPARATOR_WIDTH,
		height = SEPARATOR_SIZE,
		component = View {
			top = 0,
			left = (SEPARATOR_WIDTH - SEPARATOR_SIZE) / 2,
			width = SEPARATOR_SIZE,
			height = SEPARATOR_SIZE,
			border_radius = SEPARATOR_SIZE / 2,
			background_color = "#E94560FF",
		},
	}
end

-- Splits text into 2 parts close to the middle, at a space if there is one. The third value
-- tells if parts were separated by a space.
local function split(text, length)
	local middle = utf8.offset(text, length // 2 + 1)
	local space = string.find(text, " ", middle, true)
	if space ~= nil then
		return string.sub(text, 1, space - 1), string.sub(text, space + 1), true
	end
	return string.sub(text, 1, middle - 1), string.sub(text, middle), false
end

-- Adds segments for the text.
--
-- A text is rendered to a single texture, so a text that is a lot longer than the screen
-- needs to be split. Every segment is at most as wide as the screen. That way only a few of
-- them are a part of the scene at any time, no matter how long the text is.
--
-- - `gap` - space after the text.
local function add_text(segments, text, gap)
	local length = utf8.len(text)
	if length == nil or length == 0 then
		return
	end
	if length <= MAX_MEASURED_CHARACTERS then
		local component = Text { text = text, font_size = FONT_SIZE, color = "#FFFFFFFF" }
		local size = smelter.measure(component)
		if size.width <= output.width or length == 1 then
			table.insert(segments, {
				width = math.max(size.width, 1) + gap,
				height = size.height,
				component = component,
			})
			return
		end
	end

	local first, second, is_space_between = split(text, length)
	add_text(segments, first, if is_space_between then SPACE_WIDTH else 0)
	add_text(segments, second, gap)
end

local function new_segments(texts)
	local segments = {}
	for _, text in texts do
		local before = #segments
		-- A single line without leading and trailing spaces.
		text = string.gsub(tostring(text), "%s+", " ")
		text = string.match(text, "^ ?(.-) ?$")
		add_text(segments, text, 0)
		if #segments > before then
			table.insert(segments, separator())
		end
	end
	return segments
end

---------------------------------------------------------------------------------------------------
-- State
---------------------------------------------------------------------------------------------------

local segments = {}
-- Index of the segment that shows up next.
local next_segment = 1
-- Time when the left edge of the next segment is at the right edge of the screen.
local next_time = smelter.time()
local next_timer = nil

-- Segments that are a part of the scene:
-- { id, segment, left, duration, end_time, is_moving }
local items = {}
local next_id = 1

local add_items

local function schedule()
	if next_timer ~= nil then
		next_timer:cancel()
		next_timer = nil
	end
	if #segments > 0 then
		next_time = math.max(next_time, smelter.time())
		next_timer = smelter.at(next_time, add_items)
	end
end

-- Adds segments that are on the screen since the last frame.
--
-- A new item can't be added with a transition, because transition needs a previous state. It
-- is added without one in the place where it should be right now. The transition starts on
-- the next frame, from that place.
function add_items()
	local now = smelter.time()
	local added = {}

	-- Items that finished their transition are not visible anymore.
	for index = #items, 1, -1 do
		if items[index].end_time <= now then
			table.remove(items, index)
		end
	end

	-- More than one segment can show up between 2 frames.
	while next_time <= now do
		local segment = segments[next_segment]
		local left = output.width - (now - next_time) * SPEED / 1000
		local duration = (left + segment.width) / SPEED * 1000
		if duration > 0 then
			local item = {
				id = next_id,
				segment = segment,
				left = left,
				duration = duration,
				end_time = now + duration,
				is_moving = false,
			}
			next_id += 1
			table.insert(items, item)
			table.insert(added, item)
		end
		next_time += segment.width / SPEED * 1000
		next_segment = next_segment % #segments + 1
	end

	if #added > 0 then
		smelter.after(0, function()
			for _, item in added do
				item.is_moving = true
			end
		end)
	end
	next_timer = smelter.at(next_time, add_items)
end

local function update_texts(props)
	local texts = if type(props) == "table" and type(props.texts) == "table" then props.texts else {}
	segments = new_segments(texts)
	next_segment = 1
	schedule()
end

update_texts(smelter.props())
smelter.on("props", update_texts)

---------------------------------------------------------------------------------------------------
-- Scene
---------------------------------------------------------------------------------------------------

local function item_view(item)
	local segment = item.segment
	return View {
		id = `ticker_{item.id}`,
		top = (BAR_HEIGHT - segment.height) / 2,
		-- The item moves with a constant speed until it is behind the left edge.
		left = if item.is_moving then -segment.width else item.left,
		width = segment.width,
		height = segment.height,
		transition = if item.is_moving then { duration_ms = item.duration } else nil,
		segment.component,
	}
end

return function()
	local views = {}
	for _, item in items do
		table.insert(views, item_view(item))
	end

	local background = nil
	for _, input in smelter.inputs() do
		if input.video == "playing" then
			background = Rescaler { mode = "fill", InputStream { input_id = input.id } }
			break
		end
	end

	return View {
		background_color = "#1B1B2FFF",
		background,
		View {
			bottom = 0,
			left = 0,
			width = output.width,
			height = BAR_HEIGHT,
			background_color = "#101020D0",
			children = views,
		},
	}
end
