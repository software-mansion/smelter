import type React from 'react';
import type * as Api from '../api.js';
import type { BoxShadow, Transition } from './common.js';
import { intoApiBoxShadow, intoApiTransition } from './common.js';
import type { ComponentBaseProps, SceneComponent } from '../component.js';
import { createSmelterComponent, sceneComponentIntoApi } from '../component.js';

export type RescalerStyleProps = {
  /**
   * (**default=`"fit"`**) Rescale mode:
   */
  rescaleMode?: Api.RescaleMode;
  /**
   * (**default=`"center"`**) Horizontal alignment.
   */
  horizontalAlign?: Api.HorizontalAlign;
  /**
   * (**default=`"center"`**) Vertical alignment.
   */
  verticalAlign?: Api.VerticalAlign;
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
   * Distance in pixels between this component's bottom edge and its parent's bottom edge.
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
   * (**default=`0.0`**) Radius of a rounded corner.
   */
  borderRadius?: number;
  /**
   * (**default=`0.0`**) Border width.
   */
  borderWidth?: number;
  /**
   * (**default=`"#00000000"`**) Border color in `RGB` or `RGBA` format.
   */
  borderColor?: string;
  /**
   * Properties of the BoxShadow applied to the container.
   */
  boxShadow?: BoxShadow[];
};

export type RescalerProps = ComponentBaseProps & {
  /**
   * Single component child.
   */
  children: React.ReactElement | string | number;
  /**
   * Rescaler styling properties
   */
  style?: RescalerStyleProps;
  /**
   * Defines how this component will behave during a scene update. This will only have an
   * effect if the previous scene already contained a `Rescaler` component with the same id.
   */
  transition?: Transition;
};

const Rescaler = createSmelterComponent<RescalerProps>(sceneBuilder);

function sceneBuilder(
  { id, style, transition }: RescalerProps,
  children: SceneComponent[]
): Api.Component {
  if (children?.length !== 1) {
    throw new Error('Exactly one child is required for Rescaler component');
  }

  return {
    type: 'rescaler',
    id: id,
    child: sceneComponentIntoApi(children[0]),
    mode: style?.rescaleMode,
    horizontal_align: style?.horizontalAlign,
    vertical_align: style?.verticalAlign,
    width: style?.width,
    height: style?.height,
    top: style?.top,
    bottom: style?.bottom,
    left: style?.left,
    right: style?.right,
    rotation: style?.rotation,
    transition: transition && intoApiTransition(transition),
    border_radius: style?.borderRadius,
    border_width: style?.borderWidth,
    border_color: style?.borderColor,
    box_shadow: style?.boxShadow && intoApiBoxShadow(style.boxShadow),
  };
}

export default Rescaler;
