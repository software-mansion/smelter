-- Chat on the right side of the screen. Every second a new message shows up at the bottom
-- and moves older messages up.
--
-- Messages come from props and nothing is assumed about them: a message can be a single
-- word or a few paragraphs.

local WIDTH = 400
local MARGIN = 24
local GAP = 10
local PADDING = 12
local BORDER_WIDTH = 2
local FONT_SIZE = 22
local INTERVAL_MS = 1000
local SLIDE_MS = 250

-- `width` and `height` of a View are sizes of its content, without padding and border.
local TEXT_WIDTH = WIDTH - 2 * (PADDING + BORDER_WIDTH)
-- Height of the part of the chat that is on the screen.
local VISIBLE_HEIGHT = smelter.output.height - MARGIN

-- Messages in the scene, the oldest one first: { height, component }
local messages = {}
-- Index of the text from props that shows up next.
local next_text = 1
-- Should the next change of the chat be animated.
local animate = true

-- A View without `height` takes all the space its parent has left, so a message needs to
-- know how high its text is.
local function new_message(text)
	-- Width is defined, so the text is wrapped and its height depends on the number of lines.
	local label = Text {
		text = text,
		font_size = FONT_SIZE,
		line_height = FONT_SIZE * 1.25,
		width = TEXT_WIDTH,
		wrap = "word",
		color = "#FFFFFFFF",
	}
	local component = View {
		width = TEXT_WIDTH,
		height = smelter.measure(label).height,
		padding = PADDING,
		border_width = BORDER_WIDTH,
		border_color = "#FFFFFF50",
		border_radius = 14,
		background_color = "#14142880",
		label,
	}
	return {
		-- For a View it is the size with padding and border.
		height = smelter.measure(component).height,
		component = component,
	}
end

local function chat_height()
	local height = 0
	for index, message in messages do
		height += message.height + (if index > 1 then GAP else 0)
	end
	return height
end

-- Removes messages that moved above the top edge of the screen.
--
-- It can't be a part of the update that adds a message. Removing a message changes position
-- of the other ones inside the chat, so the size of the chat needs to change in the same
-- moment, without a transition.
local function remove_hidden_messages()
	local height = chat_height()
	while #messages > 1 and height - messages[1].height - GAP >= VISIBLE_HEIGHT do
		height -= messages[1].height + GAP
		table.remove(messages, 1)
		animate = false
	end
end

smelter.every(INTERVAL_MS, function()
	local props = smelter.props()
	local texts = if type(props) == "table" and type(props.messages) == "table" then props.messages else {}
	if #texts == 0 then
		return
	end
	next_text = if next_text > #texts then 1 else next_text
	table.insert(messages, new_message(tostring(texts[next_text])))
	next_text += 1

	animate = true
	smelter.after(2 * SLIDE_MS, remove_hidden_messages)
end)

return function()
	local children = {}
	for index, message in messages do
		if index > 1 then
			table.insert(children, View { height = GAP })
		end
		table.insert(children, message.component)
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
		-- The chat is attached to the bottom of the screen and messages are placed from its
		-- top. When a message is added, the chat gets higher: its top edge moves up along with
		-- old messages, and the new one, which starts below the bottom edge of the chat,
		-- becomes visible.
		View {
			id = "chat",
			bottom = MARGIN,
			right = MARGIN,
			width = WIDTH,
			height = chat_height(),
			direction = "column",
			transition = if animate then { duration_ms = SLIDE_MS } else nil,
			children = children,
		},
	}
end
