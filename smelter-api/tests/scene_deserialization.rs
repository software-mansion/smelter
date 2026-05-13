use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use smelter_api::*;
use smelter_render::scene;

fn component_id(id: &str) -> scene::ComponentId {
    scene::ComponentId(id.into())
}

fn input_stream(id: Option<&str>, input_id: &str) -> scene::Component {
    scene::Component::InputStream(scene::InputStreamComponent {
        id: id.map(component_id),
        input_id: smelter_render::InputId(input_id.into()),
    })
}

fn view_default() -> scene::ViewComponent {
    scene::ViewComponent::default()
}

fn rescaler_default(child: scene::Component) -> scene::RescalerComponent {
    scene::RescalerComponent {
        child: Box::new(child),
        ..scene::RescalerComponent::default()
    }
}

fn tiles_default() -> scene::TilesComponent {
    scene::TilesComponent::default()
}

fn text_default(text: &str, font_size: f32) -> scene::TextComponent {
    scene::TextComponent {
        id: None,
        text: text.into(),
        font_size,
        line_height: font_size,
        color: scene::RGBAColor(255, 255, 255, 255),
        font_family: Arc::from("Verdana"),
        style: scene::TextStyle::Normal,
        align: scene::HorizontalAlign::Left,
        weight: scene::TextWeight::Normal,
        wrap: scene::TextWrap::None,
        background_color: scene::RGBAColor(0, 0, 0, 0),
        dimensions: scene::TextDimensions::Fitted {
            max_width: smelter_render::MAX_NODE_RESOLUTION.width as f32,
            max_height: smelter_render::MAX_NODE_RESOLUTION.height as f32,
        },
    }
}

#[track_caller]
fn check(raw: serde_json::Value, expected: scene::Component) {
    let video = raw.get("video").unwrap().clone();
    let api_scene: VideoScene = serde_json::from_value(video).unwrap();
    let actual: scene::Component = api_scene.try_into().unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn view_empty() {
    check(
        json!({ "video": { "root": { "type": "view" } } }),
        scene::Component::View(view_default()),
    );
}

#[test]
fn view_with_background_color() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "view",
                    "background_color": "#FF0000FF",
                    "children": [
                        {
                            "type": "view",
                            "top": 50,
                            "right": 50,
                            "width": 400,
                            "height": 200,
                            "background_color": "#00FF00FF"
                        }
                    ]
                }
            }
        }),
        scene::Component::View(scene::ViewComponent {
            background_color: scene::RGBAColor(255, 0, 0, 255),
            children: vec![scene::Component::View(scene::ViewComponent {
                position: scene::Position::Absolute(scene::AbsolutePosition {
                    width: Some(400.0),
                    height: Some(200.0),
                    position_horizontal: scene::HorizontalPosition::RightOffset(50.0),
                    position_vertical: scene::VerticalPosition::TopOffset(50.0),
                    rotation_degrees: 0.0,
                }),
                background_color: scene::RGBAColor(0, 255, 0, 255),
                ..view_default()
            })],
            ..view_default()
        }),
    );
}

#[test]
fn view_border_radius_border_box_shadow() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "view",
                    "background_color": "#FFFF00FF",
                    "children": [
                        {
                            "type": "view",
                            "background_color": "#FF0000FF",
                            "top": 50,
                            "left": 50,
                            "width": 400,
                            "height": 200,
                            "border_radius": 50,
                            "border_width": 20,
                            "border_color": "#FFFFFFFF",
                            "box_shadow": [
                                {
                                    "offset_x": 60,
                                    "offset_y": 30,
                                    "blur_radius": 30,
                                    "color": "#00FF00FF"
                                }
                            ]
                        }
                    ]
                }
            }
        }),
        scene::Component::View(scene::ViewComponent {
            background_color: scene::RGBAColor(255, 255, 0, 255),
            children: vec![scene::Component::View(scene::ViewComponent {
                background_color: scene::RGBAColor(255, 0, 0, 255),
                position: scene::Position::Absolute(scene::AbsolutePosition {
                    width: Some(400.0),
                    height: Some(200.0),
                    position_horizontal: scene::HorizontalPosition::LeftOffset(50.0),
                    position_vertical: scene::VerticalPosition::TopOffset(50.0),
                    rotation_degrees: 0.0,
                }),
                border_radius: scene::BorderRadius::new_with_radius(50.0),
                border_width: 20.0,
                border_color: scene::RGBAColor(255, 255, 255, 255),
                box_shadow: vec![scene::BoxShadow {
                    offset_x: 60.0,
                    offset_y: 30.0,
                    blur_radius: 30.0,
                    color: scene::RGBAColor(0, 255, 0, 255),
                }],
                ..view_default()
            })],
            ..view_default()
        }),
    );
}

#[test]
fn view_overflow_fit() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "view",
                    "overflow": "fit",
                    "children": [
                        { "type": "input_stream", "input_id": "input_1" }
                    ]
                }
            }
        }),
        scene::Component::View(scene::ViewComponent {
            overflow: scene::Overflow::Fit,
            children: vec![input_stream(None, "input_1")],
            ..view_default()
        }),
    );
}

#[test]
fn view_overflow_hidden_and_visible() {
    check(
        json!({ "video": { "root": { "type": "view", "overflow": "hidden" } } }),
        scene::Component::View(scene::ViewComponent {
            overflow: scene::Overflow::Hidden,
            ..view_default()
        }),
    );
    check(
        json!({ "video": { "root": { "type": "view", "overflow": "visible" } } }),
        scene::Component::View(scene::ViewComponent {
            overflow: scene::Overflow::Visible,
            ..view_default()
        }),
    );
}

#[test]
fn view_direction_row_and_column() {
    check(
        json!({ "video": { "root": { "type": "view", "direction": "row" } } }),
        scene::Component::View(scene::ViewComponent {
            direction: scene::ViewChildrenDirection::Row,
            ..view_default()
        }),
    );
    check(
        json!({ "video": { "root": { "type": "view", "direction": "column" } } }),
        scene::Component::View(scene::ViewComponent {
            direction: scene::ViewChildrenDirection::Column,
            ..view_default()
        }),
    );
}

#[test]
fn view_nested_padding_static_children() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "view",
                    "background_color": "red",
                    "direction": "row",
                    "children": [
                        {
                            "type": "view",
                            "border_width": 10,
                            "border_color": "blue",
                            "children": []
                        },
                        {
                            "type": "view",
                            "padding_top": 20,
                            "padding_left": 20,
                            "border_width": 10,
                            "border_color": "blue",
                            "children": [
                                {
                                    "type": "view",
                                    "padding_vertical": 20,
                                    "padding_left": 20,
                                    "padding_right": 40,
                                    "border_width": 10,
                                    "border_color": "green",
                                    "background_color": "blue",
                                    "children": [
                                        {
                                            "type": "view",
                                            "width": 150,
                                            "height": 150,
                                            "padding_left": 80,
                                            "border_width": 10,
                                            "border_color": "magenta",
                                            "background_color": "yellow",
                                            "children": []
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            }
        }),
        scene::Component::View(scene::ViewComponent {
            background_color: scene::RGBAColor(255, 0, 0, 255),
            direction: scene::ViewChildrenDirection::Row,
            children: vec![
                scene::Component::View(scene::ViewComponent {
                    border_width: 10.0,
                    border_color: scene::RGBAColor(0, 0, 255, 255),
                    ..view_default()
                }),
                scene::Component::View(scene::ViewComponent {
                    padding: scene::Padding {
                        top: 20.0,
                        right: 0.0,
                        bottom: 0.0,
                        left: 20.0,
                    },
                    border_width: 10.0,
                    border_color: scene::RGBAColor(0, 0, 255, 255),
                    children: vec![scene::Component::View(scene::ViewComponent {
                        padding: scene::Padding {
                            top: 20.0,
                            right: 40.0,
                            bottom: 20.0,
                            left: 20.0,
                        },
                        border_width: 10.0,
                        border_color: scene::RGBAColor(0, 128, 0, 255),
                        background_color: scene::RGBAColor(0, 0, 255, 255),
                        children: vec![scene::Component::View(scene::ViewComponent {
                            position: scene::Position::Static {
                                width: Some(150.0),
                                height: Some(150.0),
                            },
                            padding: scene::Padding {
                                top: 0.0,
                                right: 0.0,
                                bottom: 0.0,
                                left: 80.0,
                            },
                            border_width: 10.0,
                            border_color: scene::RGBAColor(255, 0, 255, 255),
                            background_color: scene::RGBAColor(255, 255, 0, 255),
                            ..view_default()
                        })],
                        ..view_default()
                    })],
                    ..view_default()
                }),
            ],
            ..view_default()
        }),
    );
}

#[test]
fn rescaler_fit_input_stream() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "rescaler",
                    "mode": "fit",
                    "top": 90,
                    "left": 160,
                    "width": 320,
                    "height": 180,
                    "child": { "type": "input_stream", "input_id": "input_1" }
                }
            }
        }),
        scene::Component::Rescaler(scene::RescalerComponent {
            mode: scene::RescaleMode::Fit,
            position: scene::Position::Absolute(scene::AbsolutePosition {
                width: Some(320.0),
                height: Some(180.0),
                position_horizontal: scene::HorizontalPosition::LeftOffset(160.0),
                position_vertical: scene::VerticalPosition::TopOffset(90.0),
                rotation_degrees: 0.0,
            }),
            ..rescaler_default(input_stream(None, "input_1"))
        }),
    );
}

#[test]
fn rescaler_fill_input_stream_align_top_left() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "rescaler",
                    "mode": "fill",
                    "horizontal_align": "left",
                    "vertical_align": "top",
                    "top": 90,
                    "left": 160,
                    "width": 320,
                    "height": 180,
                    "child": { "type": "input_stream", "input_id": "input_1" }
                }
            }
        }),
        scene::Component::Rescaler(scene::RescalerComponent {
            mode: scene::RescaleMode::Fill,
            horizontal_align: scene::HorizontalAlign::Left,
            vertical_align: scene::VerticalAlign::Top,
            position: scene::Position::Absolute(scene::AbsolutePosition {
                width: Some(320.0),
                height: Some(180.0),
                position_horizontal: scene::HorizontalPosition::LeftOffset(160.0),
                position_vertical: scene::VerticalPosition::TopOffset(90.0),
                rotation_degrees: 0.0,
            }),
            ..rescaler_default(input_stream(None, "input_1"))
        }),
    );
}

#[test]
fn rescaler_border_radius_box_shadow() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "rescaler",
                    "top": 50,
                    "left": 50,
                    "width": 400,
                    "height": 200,
                    "border_radius": 50,
                    "box_shadow": [
                        {
                            "offset_x": 60,
                            "offset_y": 30,
                            "blur_radius": 30,
                            "color": "#00FF00FF"
                        }
                    ],
                    "child": { "type": "view", "background_color": "#FF0000FF" }
                }
            }
        }),
        scene::Component::Rescaler(scene::RescalerComponent {
            position: scene::Position::Absolute(scene::AbsolutePosition {
                width: Some(400.0),
                height: Some(200.0),
                position_horizontal: scene::HorizontalPosition::LeftOffset(50.0),
                position_vertical: scene::VerticalPosition::TopOffset(50.0),
                rotation_degrees: 0.0,
            }),
            border_radius: scene::BorderRadius::new_with_radius(50.0),
            box_shadow: vec![scene::BoxShadow {
                offset_x: 60.0,
                offset_y: 30.0,
                blur_radius: 30.0,
                color: scene::RGBAColor(0, 255, 0, 255),
            }],
            ..rescaler_default(scene::Component::View(scene::ViewComponent {
                background_color: scene::RGBAColor(255, 0, 0, 255),
                ..view_default()
            }))
        }),
    );
}

#[test]
fn transition_view_cubic_bezier() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "view",
                    "children": [
                        {
                            "id": "resize_1",
                            "type": "view",
                            "width": 200,
                            "height": 200,
                            "top": 0,
                            "right": 440,
                            "transition": {
                                "duration_ms": 5000,
                                "easing_function": {
                                    "function_name": "cubic_bezier",
                                    "points": [0.83, 0.4, 0.17, 1]
                                }
                            },
                            "background_color": "#00FF00FF"
                        }
                    ]
                }
            }
        }),
        scene::Component::View(scene::ViewComponent {
            children: vec![scene::Component::View(scene::ViewComponent {
                id: Some(component_id("resize_1")),
                position: scene::Position::Absolute(scene::AbsolutePosition {
                    width: Some(200.0),
                    height: Some(200.0),
                    position_horizontal: scene::HorizontalPosition::RightOffset(440.0),
                    position_vertical: scene::VerticalPosition::TopOffset(0.0),
                    rotation_degrees: 0.0,
                }),
                transition: Some(scene::Transition {
                    duration: Duration::from_millis(5000),
                    interpolation_kind: scene::InterpolationKind::CubicBezier {
                        x1: 0.83,
                        y1: 0.4,
                        x2: 0.17,
                        y2: 1.0,
                    },
                    should_interrupt: false,
                }),
                background_color: scene::RGBAColor(0, 255, 0, 255),
                ..view_default()
            })],
            ..view_default()
        }),
    );
}

#[test]
fn transition_default_easing() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "rescaler",
                    "id": "r",
                    "width": 640,
                    "height": 360,
                    "top": 0,
                    "right": 0,
                    "transition": { "duration_ms": 10000 },
                    "child": { "type": "view", "background_color": "#00FF00FF" }
                }
            }
        }),
        scene::Component::Rescaler(scene::RescalerComponent {
            id: Some(component_id("r")),
            position: scene::Position::Absolute(scene::AbsolutePosition {
                width: Some(640.0),
                height: Some(360.0),
                position_horizontal: scene::HorizontalPosition::RightOffset(0.0),
                position_vertical: scene::VerticalPosition::TopOffset(0.0),
                rotation_degrees: 0.0,
            }),
            transition: Some(scene::Transition {
                duration: Duration::from_millis(10000),
                interpolation_kind: scene::InterpolationKind::Linear,
                should_interrupt: false,
            }),
            ..rescaler_default(scene::Component::View(scene::ViewComponent {
                background_color: scene::RGBAColor(0, 255, 0, 255),
                ..view_default()
            }))
        }),
    );
}

#[test]
fn transition_linear_with_should_interrupt() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "view",
                    "id": "v",
                    "width": 200,
                    "height": 200,
                    "top": 0,
                    "left": 0,
                    "transition": {
                        "duration_ms": 1000,
                        "easing_function": { "function_name": "linear" },
                        "should_interrupt": true
                    },
                    "background_color": "#00FF00FF"
                }
            }
        }),
        scene::Component::View(scene::ViewComponent {
            id: Some(component_id("v")),
            position: scene::Position::Absolute(scene::AbsolutePosition {
                width: Some(200.0),
                height: Some(200.0),
                position_horizontal: scene::HorizontalPosition::LeftOffset(0.0),
                position_vertical: scene::VerticalPosition::TopOffset(0.0),
                rotation_degrees: 0.0,
            }),
            transition: Some(scene::Transition {
                duration: Duration::from_millis(1000),
                interpolation_kind: scene::InterpolationKind::Linear,
                should_interrupt: true,
            }),
            background_color: scene::RGBAColor(0, 255, 0, 255),
            ..view_default()
        }),
    );
}

#[test]
fn transition_bounce() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "view",
                    "id": "v",
                    "width": 200,
                    "height": 200,
                    "top": 0,
                    "left": 0,
                    "transition": {
                        "duration_ms": 500,
                        "easing_function": { "function_name": "bounce" }
                    }
                }
            }
        }),
        scene::Component::View(scene::ViewComponent {
            id: Some(component_id("v")),
            position: scene::Position::Absolute(scene::AbsolutePosition {
                width: Some(200.0),
                height: Some(200.0),
                position_horizontal: scene::HorizontalPosition::LeftOffset(0.0),
                position_vertical: scene::VerticalPosition::TopOffset(0.0),
                rotation_degrees: 0.0,
            }),
            transition: Some(scene::Transition {
                duration: Duration::from_millis(500),
                interpolation_kind: scene::InterpolationKind::Bounce,
                should_interrupt: false,
            }),
            ..view_default()
        }),
    );
}

#[test]
fn tiles_three_inputs() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "tiles",
                    "children": [
                        { "type": "input_stream", "input_id": "input_1" },
                        { "type": "input_stream", "input_id": "input_2" },
                        { "type": "input_stream", "input_id": "input_3" }
                    ],
                    "background_color": "#333333FF"
                }
            }
        }),
        scene::Component::Tiles(scene::TilesComponent {
            children: vec![
                input_stream(None, "input_1"),
                input_stream(None, "input_2"),
                input_stream(None, "input_3"),
            ],
            background_color: scene::RGBAColor(0x33, 0x33, 0x33, 255),
            ..tiles_default()
        }),
    );
}

#[test]
fn tiles_aspect_ratio_portrait() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "tiles",
                    "tile_aspect_ratio": "1:2",
                    "children": [
                        { "type": "input_stream", "input_id": "input_1" }
                    ],
                    "background_color": "#333333FF"
                }
            }
        }),
        scene::Component::Tiles(scene::TilesComponent {
            tile_aspect_ratio: (1, 2),
            children: vec![input_stream(None, "input_1")],
            background_color: scene::RGBAColor(0x33, 0x33, 0x33, 255),
            ..tiles_default()
        }),
    );
}

#[test]
fn tiles_align_top_left() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "tiles",
                    "vertical_align": "top",
                    "horizontal_align": "left",
                    "margin": 10,
                    "padding": 4,
                    "children": [
                        { "type": "input_stream", "input_id": "input_1" }
                    ]
                }
            }
        }),
        scene::Component::Tiles(scene::TilesComponent {
            vertical_align: scene::VerticalAlign::Top,
            horizontal_align: scene::HorizontalAlign::Left,
            margin: 10.0,
            padding: 4.0,
            children: vec![input_stream(None, "input_1")],
            ..tiles_default()
        }),
    );
}

#[test]
fn tiles_with_transition_nested_inputs() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "tiles",
                    "id": "tiles",
                    "transition": { "duration_ms": 500 },
                    "children": [
                        { "type": "input_stream", "input_id": "input_1", "id": "input_1" },
                        { "type": "input_stream", "input_id": "input_2", "id": "input_2" }
                    ]
                }
            }
        }),
        scene::Component::Tiles(scene::TilesComponent {
            id: Some(component_id("tiles")),
            transition: Some(scene::Transition {
                duration: Duration::from_millis(500),
                interpolation_kind: scene::InterpolationKind::Linear,
                should_interrupt: false,
            }),
            children: vec![
                input_stream(Some("input_1"), "input_1"),
                input_stream(Some("input_2"), "input_2"),
            ],
            ..tiles_default()
        }),
    );
}

#[test]
fn text_align_center() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "text",
                    "text": "Example text",
                    "font_size": 100,
                    "font_family": "Inter",
                    "align": "center",
                    "width": 1000,
                    "height": 200
                }
            }
        }),
        scene::Component::Text(scene::TextComponent {
            font_family: Arc::from("Inter"),
            align: scene::HorizontalAlign::Center,
            dimensions: scene::TextDimensions::Fixed {
                width: 1000.0,
                height: 200.0,
            },
            ..text_default("Example text", 100.0)
        }),
    );
}

#[test]
fn text_bold_right_aligned() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "text",
                    "text": "Example text",
                    "font_size": 100,
                    "font_family": "Inter",
                    "align": "right",
                    "weight": "bold",
                    "width": 1000,
                    "height": 200
                }
            }
        }),
        scene::Component::Text(scene::TextComponent {
            font_family: Arc::from("Inter"),
            align: scene::HorizontalAlign::Right,
            weight: scene::TextWeight::Bold,
            dimensions: scene::TextDimensions::Fixed {
                width: 1000.0,
                height: 200.0,
            },
            ..text_default("Example text", 100.0)
        }),
    );
}

#[test]
fn text_wrap_word_with_style_and_line_height() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "text",
                    "text": "Lorem ipsum",
                    "font_size": 50,
                    "line_height": 60,
                    "style": "italic",
                    "wrap": "word",
                    "max_width": 800,
                    "max_height": 400,
                    "color": "#FFFFFFFF",
                    "background_color": "#000000FF"
                }
            }
        }),
        scene::Component::Text(scene::TextComponent {
            line_height: 60.0,
            style: scene::TextStyle::Italic,
            wrap: scene::TextWrap::Word,
            dimensions: scene::TextDimensions::Fitted {
                max_width: 800.0,
                max_height: 400.0,
            },
            color: scene::RGBAColor(255, 255, 255, 255),
            background_color: scene::RGBAColor(0, 0, 0, 255),
            ..text_default("Lorem ipsum", 50.0)
        }),
    );
}

#[test]
fn image_jpeg_as_root() {
    check(
        json!({
            "video": { "root": { "type": "image", "image_id": "image_jpeg" } }
        }),
        scene::Component::Image(scene::ImageComponent {
            id: None,
            image_id: smelter_render::RendererId("image_jpeg".into()),
            width: None,
            height: None,
        }),
    );
}

#[test]
fn image_with_id_and_dimensions() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "image",
                    "id": "gif",
                    "image_id": "image_gif1",
                    "width": 320,
                    "height": 240
                }
            }
        }),
        scene::Component::Image(scene::ImageComponent {
            id: Some(component_id("gif")),
            image_id: smelter_render::RendererId("image_gif1".into()),
            width: Some(320.0),
            height: Some(240.0),
        }),
    );
}

#[test]
fn shader_with_inputs_no_params() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "shader",
                    "shader_id": "base_params_plane_id",
                    "resolution": { "width": 640, "height": 360 },
                    "children": [
                        { "type": "input_stream", "input_id": "input_1" },
                        { "type": "input_stream", "input_id": "input_2" }
                    ]
                }
            }
        }),
        scene::Component::Shader(scene::ShaderComponent {
            id: None,
            shader_id: smelter_render::RendererId("base_params_plane_id".into()),
            shader_param: None,
            size: scene::Size {
                width: 640.0,
                height: 360.0,
            },
            children: vec![input_stream(None, "input_1"), input_stream(None, "input_2")],
        }),
    );
}

#[test]
fn shader_param_full_enum_coverage() {
    check(
        json!({
            "video": {
                "root": {
                    "type": "shader",
                    "shader_id": "custom",
                    "resolution": { "width": 640, "height": 360 },
                    "shader_param": {
                        "type": "struct",
                        "value": [
                            { "field_name": "intensity", "type": "f32", "value": 0.5 },
                            { "field_name": "count", "type": "u32", "value": 7 },
                            { "field_name": "offset", "type": "i32", "value": -3 },
                            {
                                "field_name": "weights",
                                "type": "list",
                                "value": [
                                    { "type": "f32", "value": 0.25 },
                                    { "type": "f32", "value": 0.5 },
                                    { "type": "f32", "value": 0.125 }
                                ]
                            }
                        ]
                    },
                    "children": []
                }
            }
        }),
        scene::Component::Shader(scene::ShaderComponent {
            id: None,
            shader_id: smelter_render::RendererId("custom".into()),
            shader_param: Some(scene::ShaderParam::Struct(vec![
                scene::ShaderParamStructField {
                    field_name: "intensity".into(),
                    value: scene::ShaderParam::F32(0.5),
                },
                scene::ShaderParamStructField {
                    field_name: "count".into(),
                    value: scene::ShaderParam::U32(7),
                },
                scene::ShaderParamStructField {
                    field_name: "offset".into(),
                    value: scene::ShaderParam::I32(-3),
                },
                scene::ShaderParamStructField {
                    field_name: "weights".into(),
                    value: scene::ShaderParam::List(vec![
                        scene::ShaderParam::F32(0.25),
                        scene::ShaderParam::F32(0.5),
                        scene::ShaderParam::F32(0.125),
                    ]),
                },
            ])),
            size: scene::Size {
                width: 640.0,
                height: 360.0,
            },
            children: vec![],
        }),
    );
}
