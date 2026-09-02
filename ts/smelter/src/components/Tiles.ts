import type * as Api from '../api.js';
import type { Transition } from './common.js';
import { intoApiTransition } from './common.js';
import type { ComponentBaseProps, SceneComponent } from '../component.js';
import { createSmelterComponent, sceneComponentIntoApi } from '../component.js';

export type TilesStyleProps = {
  /**
   * Width of a component in pixels. Exact behavior might be different based on the parent
   * component:
   * - If the parent component is a layout, check sections "Absolute positioning" and "Static
   * positioning" of that component.
   * - If the parent component is not a layout, then this field is required.
   */
  width?: number;
  /**
   * Height of a component in pixels. Exact behavior might be different based on the parent
   * component:
   * - If the parent component is a layout, check sections "Absolute positioning" and "Static
   * positioning" of that component.
   * - If the parent component is not a layout, then this field is required.
   */
  height?: number;
  /**
   * Background color in `RGB` or `RGBA` format. Defaults to `"#00000000"`.
   */
  backgroundColor?: string;
  /**
   * Aspect ratio of a tile in `"W:H"` format, where W and H are integers. Defaults to `"16:9"`.
   */
  tileAspectRatio?: Api.AspectRatio | null;
  /**
   * Margin of each tile in pixels. Defaults to `0`.
   */
  margin?: number;
  /**
   * Padding on each tile in pixels. Defaults to `0`.
   */
  padding?: number;
  /**
   * Horizontal alignment of tiles. Defaults to `"center"`.
   */
  horizontalAlign?: Api.HorizontalAlign;
  /**
   * Vertical alignment of tiles. Defaults to `"center"`.
   */
  verticalAlign?: Api.VerticalAlign;
};

export type TilesProps = ComponentBaseProps & {
  /**
   * Tiles styling properties
   */
  style?: TilesStyleProps;
  /**
   * Defines how this component will behave during a scene update. This will only have an
   * effect if the previous scene already contained a `Tiles` component with the same id.
   */
  transition?: Transition;
};

const Tiles = createSmelterComponent<TilesProps>(sceneBuilder);

function sceneBuilder(
  { id, style, transition }: TilesProps,
  children: SceneComponent[]
): Api.Component {
  return {
    type: 'tiles',
    id: id,
    children: children.map(sceneComponentIntoApi),
    width: style?.width,
    height: style?.height,
    background_color: style?.backgroundColor,
    tile_aspect_ratio: style?.tileAspectRatio,
    margin: style?.margin,
    padding: style?.padding,
    horizontal_align: style?.horizontalAlign,
    vertical_align: style?.verticalAlign,
    transition: transition && intoApiTransition(transition),
  };
}

export default Tiles;
