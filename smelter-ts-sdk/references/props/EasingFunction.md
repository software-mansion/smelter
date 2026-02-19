# EasingFunction

Defines the interpolation curve for transition animations.

```tsx
type EasingFunction =
  | "linear"
  | "bounce"
  | {
      functionName: "cubic_bezier";
      points: [number, number, number, number];
    };
```

## Values

### "linear" (default)
Linear interpolation from start to end value.

### "bounce"
Bounce effect at the end of the transition.

### cubic_bezier
Custom cubic Bézier easing curve.

```tsx
{
  functionName: "cubic_bezier",
  points: [x1, y1, x2, y2]  // values in [0, 1] range
}
```

The `points` array defines two control points `(x1, y1)` and `(x2, y2)`. Result is clamped to `[0, 1]`.

## Usage

```tsx
// Simple linear
transition={{ durationMs: 300, easingFunction: "linear" }}

// Bounce
transition={{ durationMs: 500, easingFunction: "bounce" }}

// Custom ease-in-out (similar to CSS ease-in-out)
transition={{ durationMs: 400, easingFunction: { functionName: "cubic_bezier", points: [0.42, 0, 0.58, 1] } }}
```
