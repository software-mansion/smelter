import type * as Api from '../api.js';
import type { ComponentBaseProps, SceneComponent } from '../component.js';
import { createSmelterComponent, DEFAULT_FONT_SIZE } from '../component.js';

export type TextStyleProps = {
  /**
   * Width of a texture that text will be rendered on. If not provided, the resulting texture
   * will be sized based on the defined text but limited to `max_width` value.
   */
  width?: number;
  /**
   * Height of a texture that text will be rendered on. If not provided, the resulting texture
   * will be sized based on the defined text but limited to `max_height` value.
   * It's an error to provide `height` if `width` is not defined.
   */
  height?: number;
  /**
   * Maximal `width`. Limits the width of the texture that the text will be rendered on. Value is
   * ignored if `width` is defined.
   *
   * Defaults to `7682`.
   */
  maxWidth?: number;
  /**
   * Maximal `height`. Limits the height of the texture that the text will be rendered on. Value is
   * ignored if height is defined.
   *
   * Defaults to `4320`.
   */
  maxHeight?: number;
  /**
   * Font size in pixels.
   */
  fontSize: number;
  /**
   * Distance between lines in pixels. Defaults to the value of the `font_size` property.
   */
  lineHeight?: number;
  /**
   * Font color in `RGB` or `RGBA` format. Defaults to `"#FFFFFFFF"`.
   */
  color?: string;
  /**
   * Background color in `RGB` or `RGBA` format. Defaults to `"#00000000"`.
   */
  backgroundColor?: string;
  /**
   * Font family. Provide family-name (see
   * https://www.w3.org/TR/2018/REC-css-fonts-3-20180920/#family-name-value) for a specific
   * font. "generic-family" values like e.g. "sans-serif" will not work.
   *
   * Defaults to `"Verdana"`.
   */
  fontFamily?: string;
  /**
   * Font style. The selected font needs to support the specified style. Defaults to `"normal"`.
   */
  fontStyle?: Api.TextStyle;
  /**
   * Text align. Defaults to `"left"`.
   */
  align?: Api.HorizontalAlign;
  /**
   * Text wrapping options. Defaults to `"none"`.
   */
  wrap?: Api.TextWrapMode;
  /**
   * Font weight. The selected font needs to support the specified weight. Defaults to `"normal"`.
   */
  fontWeight?: Api.TextWeight;
};

export type TextProps = ComponentBaseProps & {
  /**
   * Text content.
   */
  children?: (string | number)[] | string | number;
  /**
   * Text styling properties
   */
  style?: TextStyleProps;
};

const Text = createSmelterComponent<TextProps>(sceneBuilder);

function sceneBuilder(props: TextProps, children: SceneComponent[]): Api.Component {
  const { id, style } = props;

  return {
    type: 'text',
    id: id,
    text: children.map(child => (typeof child === 'string' ? child : String(child))).join(''),
    width: style?.width,
    height: style?.height,
    max_width: style?.maxWidth,
    max_height: style?.maxHeight,
    font_size: style?.fontSize ?? DEFAULT_FONT_SIZE,
    line_height: style?.lineHeight,
    color: style?.color,
    background_color: style?.backgroundColor,
    font_family: style?.fontFamily,
    style: style?.fontStyle,
    align: style?.align,
    wrap: style?.wrap,
    weight: style?.fontWeight,
  };
}

export default Text;
