-- Grid of all inputs that are playing, with a label on each of them and a logo in the corner.
-- Title from props shows up for a few seconds every time props are updated.

local PADDING = 16
local TITLE_DURATION_MS = 4000

local title_visible = false
local title_timer = nil

local function show_title()
	title_visible = true
	if title_timer then
		title_timer:cancel()
	end
	title_timer = smelter.after(TITLE_DURATION_MS, function()
		title_visible = false
	end)
end

show_title()
smelter.on("props", show_title)

smelter.on("video_input_eos", function(input)
	print(`input {input.id} finished at {smelter.time()}ms`)
end)

-- Image is resized to the height of 96px, width depends on the aspect ratio of the image.
local logo = Image { image_id = "logo", height = 96 }
local logo_size = smelter.measure(logo)

-- Box that is exactly as big as the text inside (plus padding).
local function text_box(props)
	local text = Text { text = props.text, font_size = props.font_size, weight = "bold" }
	local size = smelter.measure(text)
	-- Size of the box with padding, `width` and `height` of a View are sizes of its content.
	local width = size.width + 2 * PADDING
	local height = size.height + PADDING
	return View {
		top = props.top and props.top(height),
		bottom = props.bottom,
		left = props.left(width),
		width = size.width,
		height = size.height,
		padding_vertical = PADDING / 2,
		padding_horizontal = PADDING,
		border_radius = 8,
		background_color = props.background_color,
		text,
	}
end

return function()
	local tiles = {}
	local audio = { inputs = {} }
	for _, input in smelter.inputs() do
		if input.video == "playing" then
			table.insert(tiles, View {
				id = "tile_" .. input.id,
				Rescaler { InputStream { input_id = input.id } },
				text_box {
					text = `{input.id} {input.width}x{input.height}`,
					font_size = 28,
					background_color = "#000000B0",
					bottom = PADDING,
					left = function()
						return PADDING
					end,
				},
			})
		end
		if input.audio == "playing" then
			table.insert(audio.inputs, { input_id = input.id })
		end
	end

	local scene = View {
		background_color = "#1B1B2FFF",
		Tiles {
			id = "grid",
			margin = 12,
			transition = { duration_ms = 400 },
			children = tiles,
		},
		View {
			top = PADDING,
			right = PADDING,
			width = logo_size.width,
			height = logo_size.height,
			logo,
		},
		title_visible and text_box {
			text = smelter.props().title,
			font_size = 56,
			background_color = "#E94560FF",
			top = function(height)
				return (smelter.output.height - height) / 2
			end,
			left = function(width)
				return (smelter.output.width - width) / 2
			end,
		},
	}
	return scene, audio
end
