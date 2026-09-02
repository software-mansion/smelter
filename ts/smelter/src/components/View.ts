import type * as Api from '../api.js';
import type { ComponentBaseProps, SceneComponent } from '../component.js';
import { createSmelterComponent, sceneComponentIntoApi } from '../component.js';
import type { BoxShadow, Transition } from './common.js';
import { intoApiBoxShadow, intoApiTransition } from './common.js';

export type ViewStyleProps = {
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
   * Direction defines how static children are positioned inside a View component.
   */
  direction?: Api.ViewDirection;
  /**
   * Distance in pixels between this component's top edge and its parent's top edge.
   * If this field is defined, then the component will ignore a layout defined by its parent.
   */
  top?: number;
  /**
   * Distance in pixels between this component's right edge and its parent's right edge.
   * If this field is defined, this element will be absolutely positioned, instead of being
   * laid out by its parent.
   */
  right?: number;
  /**
   * Distance in pixels between the bottom edge of this component and the bottom edge of its parent.
   * If this field is defined, this element will be absolutely positioned, instead of being
   * laid out by its parent.
   */
  bottom?: number;
  /**
   * Distance in pixels between this component's left edge and its parent's left edge.
   * If this field is defined, this element will be absolutely positioned, instead of being
   * laid out by its parent.
   */
  left?: number;
  /**
   * Rotation of a component in degrees. If this field is defined, this element will be
   * absolutely positioned, instead of being laid out by its parent.
   */
  rotation?: number;
  /**
   * Controls what happens to content that is too big to fit into an area. Defaults to `"hidden"`.
   */
  overflow?: Api.Overflow;
  /**
   * Background color in `RGB` or `RGBA` format. Defaults to `"#00000000"`.
   */
  backgroundColor?: string;
  /**
   * Radius of a rounded corner. Defaults to `0.0`.
   */
  borderRadius?: number;
  /**
   * Border width. Defaults to `0.0`.
   */
  borderWidth?: number;
  /**
   * Border color in `RGB` or `RGBA` format. Defaults to `"#00000000"`.
   */
  borderColor?: string;
  /**
   * Properties of the BoxShadow applied to the container.
   */
  boxShadow?: BoxShadow[];
  /**
   * Sets padding for all sides of the component. Defaults to `0.0`.
   */
  padding?: number;
  /**
   * Sets padding for the top and bottom of the component. Defaults to `0.0`.
   */
  paddingVertical?: number;
  /**
   * Sets padding for the left and right of the component. Defaults to `0.0`.
   */
  paddingHorizontal?: number;
  /**
   * Sets padding for the top of the component. Defaults to `0.0`.
   */
  paddingTop?: number;
  /**
   * Sets padding for the right of the component. Defaults to `0.0`.
   */
  paddingRight?: number;
  /**
   * Sets padding for the bottom of the component. Defaults to `0.0`.
   */
  paddingBottom?: number;
  /**
   * Sets padding for the left of the component. Defaults to `0.0`.
   */
  paddingLeft?: number;
};

export type ViewProps = ComponentBaseProps & {
  /**
   * Component styling properties.
   */
  style?: ViewStyleProps;
  /**
   * Defines how this component will behave during a scene update. This will only have an
   * effect if the previous scene already contained a `View` component with the same id.
   */
  transition?: Transition;
};

const View = createSmelterComponent<ViewProps>(sceneBuilder);

function sceneBuilder(
  { id, style = {}, transition }: ViewProps,
  children: SceneComponent[]
): Api.Component {
  return {
    type: 'view',
    id,
    children: children.map(sceneComponentIntoApi),
    width: style.width,
    height: style.height,
    direction: style.direction,

    top: style.top,
    right: style.right,
    bottom: style.bottom,
    left: style.left,

    rotation: style.rotation,
    overflow: style.overflow,
    background_color: style.backgroundColor,
    transition: transition && intoApiTransition(transition),

    border_radius: style.borderRadius,
    border_width: style.borderWidth,
    border_color: style.borderColor,

    box_shadow: style.boxShadow && intoApiBoxShadow(style.boxShadow),

    padding: style.padding,
    padding_vertical: style.paddingVertical,
    padding_horizontal: style.paddingHorizontal,
    padding_top: style.paddingTop,
    padding_bottom: style.paddingBottom,
    padding_right: style.paddingRight,
    padding_left: style.paddingLeft,
  };
}

export default View;
