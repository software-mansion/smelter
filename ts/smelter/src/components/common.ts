import type * as Api from '../api.js';

export interface Transition {
  /**
   * Duration of a transition in milliseconds.
   */
  durationMs: number;
  /**
   * (**default=`"linear"`**) Easing function to be used for the transition.
   */
  easingFunction?: EasingFunction | null;
  /**
   * (**default=`false`**) On scene update, if there is already a transition in progress,
   * it will be interrupted and the new transition will start from the current state.
   */
  shouldInterrupt?: boolean;
}

export function intoApiTransition(transition: Transition): Api.Transition {
  return {
    duration_ms: transition.durationMs,
    easing_function: transition.easingFunction
      ? intoApiEasingFunction(transition.easingFunction)
      : undefined,
    should_interrupt: transition.shouldInterrupt,
  };
}

export type EasingFunction =
  | 'linear'
  | 'bounce'
  | { functionName: 'linear' }
  | { functionName: 'bounce' }
  | {
      functionName: 'cubic_bezier';
      points: [number, number, number, number];
    };

export function intoApiEasingFunction(easing: EasingFunction): Api.EasingFunction {
  if (easing === 'linear' || easing === 'bounce') {
    return { function_name: easing };
  } else if (
    typeof easing === 'object' &&
    (easing.functionName === 'linear' || easing.functionName == 'bounce')
  ) {
    return { function_name: easing.functionName };
  } else if (typeof easing === 'object' && easing.functionName === 'cubic_bezier') {
    return {
      function_name: 'cubic_bezier',
      points: easing.points,
    };
  } else {
    throw new Error(`Invalid Smelter.EasingFunction ${easing}`);
  }
}

export interface BoxShadow {
  offsetX?: number | null;
  offsetY?: number | null;
  color?: string | null;
  blurRadius?: number | null;
}

export function intoApiBoxShadow(boxShadows: BoxShadow[]): Api.BoxShadow[] {
  return boxShadows.map(boxShadow => ({
    offset_x: boxShadow.offsetX,
    offset_y: boxShadow.offsetY,
    color: boxShadow.color,
    blur_radius: boxShadow.blurRadius,
  }));
}
