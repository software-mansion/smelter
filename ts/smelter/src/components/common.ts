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
   * (**default=`false`**) If `true`, the ongoing transition will reset on update.
   */
  resetOnUpdate?: boolean;
}

export function intoApiTransition(transition: Transition): Api.Transition {
  return {
    duration_ms: transition.durationMs,
    easing_function: transition.easingFunction
      ? intoApiEasingFunction(transition.easingFunction)
      : undefined,
    reset_on_update: transition.resetOnUpdate,
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
