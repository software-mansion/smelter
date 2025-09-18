use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smelter_render::scene;

use crate::*;

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, PartialEq)]
pub struct RGBAColor(pub String);

impl TryFrom<RGBAColor> for scene::RGBAColor {
    type Error = TypeError;
    fn try_from(value: RGBAColor) -> std::result::Result<Self, Self::Error> {
        let s = &value.0.trim();
        if let Some(named_color) = parse_named_color(s) {
            return Ok(named_color);
        }
        if s.starts_with('#') {
            return match s.len() {
                7 => parse_hex(s),
                9 => parse_hex_rgba(s),
                _ => Err(TypeError::new(
                    "Invalid format. Color has to be in #RRGGBB or #RRGGBBAA format.",
                )),
            };
        }
        if s.starts_with("rgb(") {
            return parse_rgb(s);
        }

        if s.starts_with("rgba(") {
            return parse_rgba(s);
        }

        Err(TypeError::new("Unsupported color format."))
    }
}

fn parse_color_channel(value: &str, radix: u32) -> Result<u8, TypeError> {
    u8::from_str_radix(value.trim(), radix).map_err(|_err| {
        TypeError::new("Invalid format. Color representation is not a valid number.")
    })
}

fn parse_hex(s: &str) -> Result<scene::RGBAColor, TypeError> {
    let (r, g, b) = (&s[1..3], &s[3..5], &s[5..7]);
    Ok(scene::RGBAColor(
        parse_color_channel(r, 16)?,
        parse_color_channel(g, 16)?,
        parse_color_channel(b, 16)?,
        255,
    ))
}

fn parse_hex_rgba(s: &str) -> Result<scene::RGBAColor, TypeError> {
    let (r, g, b, a) = (&s[1..3], &s[3..5], &s[5..7], &s[7..9]);
    Ok(scene::RGBAColor(
        parse_color_channel(r, 16)?,
        parse_color_channel(g, 16)?,
        parse_color_channel(b, 16)?,
        parse_color_channel(a, 16)?,
    ))
}

fn parse_rgb(s: &str) -> Result<scene::RGBAColor, TypeError> {
    let inner_s = s.trim_start_matches("rgb(").trim_end_matches(')');
    let parts: Vec<&str> = inner_s.split(',').collect();
    match parts.as_slice() {
        [r, g, b] => Ok(scene::RGBAColor(
            parse_color_channel(r, 10)?,
            parse_color_channel(g, 10)?,
            parse_color_channel(b, 10)?,
            255,
        )),
        _ => Err(TypeError::new("Invalid RGB format.")),
    }
}

fn parse_rgba(s: &str) -> Result<scene::RGBAColor, TypeError> {
    let inner_s = s.trim_start_matches("rgba(").trim_end_matches(')');
    let parts: Vec<&str> = inner_s.split(',').collect();
    let [r, g, b, a] = parts.as_slice() else {
        return Err(TypeError::new(
            "Expected three color components and alpha channel.",
        ));
    };
    let a = a
        .parse::<f32>()
        .map_err(|_| TypeError::new("Alpha channel parsing failed."))?;
    if !(0.0..=1.0).contains(&a) {
        return Err(TypeError::new(
            "Alpha value out of range. It must be between 0.0 and 1.0",
        ));
    }
    Ok(scene::RGBAColor(
        parse_color_channel(r, 10)?,
        parse_color_channel(g, 10)?,
        parse_color_channel(b, 10)?,
        (a * 255.0).round() as u8,
    ))
}

fn parse_named_color(color_name: &str) -> Option<scene::RGBAColor> {
    match color_name {
        "aliceblue" => Some(scene::RGBAColor(240, 248, 255, 255)),
        "antiquewhite" => Some(scene::RGBAColor(250, 235, 215, 255)),
        "aqua" => Some(scene::RGBAColor(0, 255, 255, 255)),
        "aquamarine" => Some(scene::RGBAColor(127, 255, 212, 255)),
        "azure" => Some(scene::RGBAColor(240, 255, 255, 255)),
        "beige" => Some(scene::RGBAColor(245, 245, 220, 255)),
        "bisque" => Some(scene::RGBAColor(255, 228, 196, 255)),
        "black" => Some(scene::RGBAColor(0, 0, 0, 255)),
        "blanchedalmond" => Some(scene::RGBAColor(255, 235, 205, 255)),
        "blue" => Some(scene::RGBAColor(0, 0, 255, 255)),
        "blueviolet" => Some(scene::RGBAColor(138, 43, 226, 255)),
        "brown" => Some(scene::RGBAColor(165, 42, 42, 255)),
        "burlywood" => Some(scene::RGBAColor(222, 184, 135, 255)),
        "burntsienna" => Some(scene::RGBAColor(234, 126, 93, 255)),
        "cadetblue" => Some(scene::RGBAColor(95, 158, 160, 255)),
        "chartreuse" => Some(scene::RGBAColor(127, 255, 0, 255)),
        "chocolate" => Some(scene::RGBAColor(210, 105, 30, 255)),
        "coral" => Some(scene::RGBAColor(255, 127, 80, 255)),
        "cornflowerblue" => Some(scene::RGBAColor(100, 149, 237, 255)),
        "cornsilk" => Some(scene::RGBAColor(255, 248, 220, 255)),
        "crimson" => Some(scene::RGBAColor(220, 20, 60, 255)),
        "cyan" => Some(scene::RGBAColor(0, 255, 255, 255)),
        "darkblue" => Some(scene::RGBAColor(0, 0, 139, 255)),
        "darkcyan" => Some(scene::RGBAColor(0, 139, 139, 255)),
        "darkgoldenrod" => Some(scene::RGBAColor(184, 134, 11, 255)),
        "darkgray" => Some(scene::RGBAColor(169, 169, 169, 255)),
        "darkgreen" => Some(scene::RGBAColor(0, 100, 0, 255)),
        "darkgrey" => Some(scene::RGBAColor(169, 169, 169, 255)),
        "darkkhaki" => Some(scene::RGBAColor(189, 183, 107, 255)),
        "darkmagenta" => Some(scene::RGBAColor(139, 0, 139, 255)),
        "darkolivegreen" => Some(scene::RGBAColor(85, 107, 47, 255)),
        "darkorange" => Some(scene::RGBAColor(255, 140, 0, 255)),
        "darkorchid" => Some(scene::RGBAColor(153, 50, 204, 255)),
        "darkred" => Some(scene::RGBAColor(139, 0, 0, 255)),
        "darksalmon" => Some(scene::RGBAColor(233, 150, 122, 255)),
        "darkseagreen" => Some(scene::RGBAColor(143, 188, 143, 255)),
        "darkslateblue" => Some(scene::RGBAColor(72, 61, 139, 255)),
        "darkslategray" => Some(scene::RGBAColor(47, 79, 79, 255)),
        "darkslategrey" => Some(scene::RGBAColor(47, 79, 79, 255)),
        "darkturquoise" => Some(scene::RGBAColor(0, 206, 209, 255)),
        "darkviolet" => Some(scene::RGBAColor(148, 0, 211, 255)),
        "deeppink" => Some(scene::RGBAColor(255, 20, 147, 255)),
        "deepskyblue" => Some(scene::RGBAColor(0, 191, 255, 255)),
        "dimgray" => Some(scene::RGBAColor(105, 105, 105, 255)),
        "dimgrey" => Some(scene::RGBAColor(105, 105, 105, 255)),
        "dodgerblue" => Some(scene::RGBAColor(30, 144, 255, 255)),
        "firebrick" => Some(scene::RGBAColor(178, 34, 34, 255)),
        "floralwhite" => Some(scene::RGBAColor(255, 250, 240, 255)),
        "forestgreen" => Some(scene::RGBAColor(34, 139, 34, 255)),
        "fuchsia" => Some(scene::RGBAColor(255, 0, 255, 255)),
        "gainsboro" => Some(scene::RGBAColor(220, 220, 220, 255)),
        "ghostwhite" => Some(scene::RGBAColor(248, 248, 255, 255)),
        "gold" => Some(scene::RGBAColor(255, 215, 0, 255)),
        "goldenrod" => Some(scene::RGBAColor(218, 165, 32, 255)),
        "gray" => Some(scene::RGBAColor(128, 128, 128, 255)),
        "green" => Some(scene::RGBAColor(0, 128, 0, 255)),
        "greenyellow" => Some(scene::RGBAColor(173, 255, 47, 255)),
        "grey" => Some(scene::RGBAColor(128, 128, 128, 255)),
        "honeydew" => Some(scene::RGBAColor(240, 255, 240, 255)),
        "hotpink" => Some(scene::RGBAColor(255, 105, 180, 255)),
        "indianred" => Some(scene::RGBAColor(205, 92, 92, 255)),
        "indigo" => Some(scene::RGBAColor(75, 0, 130, 255)),
        "ivory" => Some(scene::RGBAColor(255, 255, 240, 255)),
        "khaki" => Some(scene::RGBAColor(240, 230, 140, 255)),
        "lavender" => Some(scene::RGBAColor(230, 230, 250, 255)),
        "lavenderblush" => Some(scene::RGBAColor(255, 240, 245, 255)),
        "lawngreen" => Some(scene::RGBAColor(124, 252, 0, 255)),
        "lemonchiffon" => Some(scene::RGBAColor(255, 250, 205, 255)),
        "lightblue" => Some(scene::RGBAColor(173, 216, 230, 255)),
        "lightcoral" => Some(scene::RGBAColor(240, 128, 128, 255)),
        "lightcyan" => Some(scene::RGBAColor(224, 255, 255, 255)),
        "lightgoldenrodyellow" => Some(scene::RGBAColor(250, 250, 210, 255)),
        "lightgray" => Some(scene::RGBAColor(211, 211, 211, 255)),
        "lightgreen" => Some(scene::RGBAColor(144, 238, 144, 255)),
        "lightgrey" => Some(scene::RGBAColor(211, 211, 211, 255)),
        "lightpink" => Some(scene::RGBAColor(255, 182, 193, 255)),
        "lightsalmon" => Some(scene::RGBAColor(255, 160, 122, 255)),
        "lightseagreen" => Some(scene::RGBAColor(32, 178, 170, 255)),
        "lightskyblue" => Some(scene::RGBAColor(135, 206, 250, 255)),
        "lightslategray" => Some(scene::RGBAColor(119, 136, 153, 255)),
        "lightslategrey" => Some(scene::RGBAColor(119, 136, 153, 255)),
        "lightsteelblue" => Some(scene::RGBAColor(176, 196, 222, 255)),
        "lightyellow" => Some(scene::RGBAColor(255, 255, 224, 255)),
        "lime" => Some(scene::RGBAColor(0, 255, 0, 255)),
        "limegreen" => Some(scene::RGBAColor(50, 205, 50, 255)),
        "linen" => Some(scene::RGBAColor(250, 240, 230, 255)),
        "magenta" => Some(scene::RGBAColor(255, 0, 255, 255)),
        "maroon" => Some(scene::RGBAColor(128, 0, 0, 255)),
        "mediumaquamarine" => Some(scene::RGBAColor(102, 205, 170, 255)),
        "mediumblue" => Some(scene::RGBAColor(0, 0, 205, 255)),
        "mediumorchid" => Some(scene::RGBAColor(186, 85, 211, 255)),
        "mediumpurple" => Some(scene::RGBAColor(147, 112, 219, 255)),
        "mediumseagreen" => Some(scene::RGBAColor(60, 179, 113, 255)),
        "mediumslateblue" => Some(scene::RGBAColor(123, 104, 238, 255)),
        "mediumspringgreen" => Some(scene::RGBAColor(0, 250, 154, 255)),
        "mediumturquoise" => Some(scene::RGBAColor(72, 209, 204, 255)),
        "mediumvioletred" => Some(scene::RGBAColor(199, 21, 133, 255)),
        "midnightblue" => Some(scene::RGBAColor(25, 25, 112, 255)),
        "mintcream" => Some(scene::RGBAColor(245, 255, 250, 255)),
        "mistyrose" => Some(scene::RGBAColor(255, 228, 225, 255)),
        "moccasin" => Some(scene::RGBAColor(255, 228, 181, 255)),
        "navajowhite" => Some(scene::RGBAColor(255, 222, 173, 255)),
        "navy" => Some(scene::RGBAColor(0, 0, 128, 255)),
        "oldlace" => Some(scene::RGBAColor(253, 245, 230, 255)),
        "olive" => Some(scene::RGBAColor(128, 128, 0, 255)),
        "olivedrab" => Some(scene::RGBAColor(107, 142, 35, 255)),
        "orange" => Some(scene::RGBAColor(255, 165, 0, 255)),
        "orangered" => Some(scene::RGBAColor(255, 69, 0, 255)),
        "orchid" => Some(scene::RGBAColor(218, 112, 214, 255)),
        "palegoldenrod" => Some(scene::RGBAColor(238, 232, 170, 255)),
        "palegreen" => Some(scene::RGBAColor(152, 251, 152, 255)),
        "paleturquoise" => Some(scene::RGBAColor(175, 238, 238, 255)),
        "palevioletred" => Some(scene::RGBAColor(219, 112, 147, 255)),
        "papayawhip" => Some(scene::RGBAColor(255, 239, 213, 255)),
        "peachpuff" => Some(scene::RGBAColor(255, 218, 185, 255)),
        "peru" => Some(scene::RGBAColor(205, 133, 63, 255)),
        "pink" => Some(scene::RGBAColor(255, 192, 203, 255)),
        "plum" => Some(scene::RGBAColor(221, 160, 221, 255)),
        "powderblue" => Some(scene::RGBAColor(176, 224, 230, 255)),
        "purple" => Some(scene::RGBAColor(128, 0, 128, 255)),
        "rebeccapurple" => Some(scene::RGBAColor(102, 51, 153, 255)),
        "red" => Some(scene::RGBAColor(255, 0, 0, 255)),
        "rosybrown" => Some(scene::RGBAColor(188, 143, 143, 255)),
        "royalblue" => Some(scene::RGBAColor(65, 105, 225, 255)),
        "saddlebrown" => Some(scene::RGBAColor(139, 69, 19, 255)),
        "salmon" => Some(scene::RGBAColor(250, 128, 114, 255)),
        "sandybrown" => Some(scene::RGBAColor(244, 164, 96, 255)),
        "seagreen" => Some(scene::RGBAColor(46, 139, 87, 255)),
        "seashell" => Some(scene::RGBAColor(255, 245, 238, 255)),
        "sienna" => Some(scene::RGBAColor(160, 82, 45, 255)),
        "silver" => Some(scene::RGBAColor(192, 192, 192, 255)),
        "skyblue" => Some(scene::RGBAColor(135, 206, 235, 255)),
        "slateblue" => Some(scene::RGBAColor(106, 90, 205, 255)),
        "slategray" => Some(scene::RGBAColor(112, 128, 144, 255)),
        "slategrey" => Some(scene::RGBAColor(112, 128, 144, 255)),
        "snow" => Some(scene::RGBAColor(255, 250, 250, 255)),
        "springgreen" => Some(scene::RGBAColor(0, 255, 127, 255)),
        "steelblue" => Some(scene::RGBAColor(70, 130, 180, 255)),
        "tan" => Some(scene::RGBAColor(210, 180, 140, 255)),
        "teal" => Some(scene::RGBAColor(0, 128, 128, 255)),
        "thistle" => Some(scene::RGBAColor(216, 191, 216, 255)),
        "tomato" => Some(scene::RGBAColor(255, 99, 71, 255)),
        "turquoise" => Some(scene::RGBAColor(64, 224, 208, 255)),
        "violet" => Some(scene::RGBAColor(238, 130, 238, 255)),
        "wheat" => Some(scene::RGBAColor(245, 222, 179, 255)),
        "white" => Some(scene::RGBAColor(255, 255, 255, 255)),
        "whitesmoke" => Some(scene::RGBAColor(245, 245, 245, 255)),
        "yellow" => Some(scene::RGBAColor(255, 255, 0, 255)),
        "yellowgreen" => Some(scene::RGBAColor(154, 205, 50, 255)),
        _ => None,
    }
}

#[test]
fn test_rgba_deserialization() {
    fn test_case(color: &str, expected: Result<scene::RGBAColor, TypeError>) {
        assert_eq!(
            scene::RGBAColor::try_from(RGBAColor(color.to_string())),
            expected
        );
    }
    test_case("#00000000", Ok(scene::RGBAColor(0, 0, 0, 0)));
    test_case("#01020304", Ok(scene::RGBAColor(1, 2, 3, 4)));
    test_case("#01FF0304", Ok(scene::RGBAColor(1, 255, 3, 4)));
    test_case("#FFffFFff", Ok(scene::RGBAColor(255, 255, 255, 255)));
    test_case(
        "#0000000G",
        Err(TypeError::new(
            "Invalid format. Color representation is not a valid number.",
        )),
    );
    test_case(
        "#000",
        Err(TypeError::new(
            "Invalid format. Color has to be in #RRGGBB or #RRGGBBAA format.",
        )),
    );
}
