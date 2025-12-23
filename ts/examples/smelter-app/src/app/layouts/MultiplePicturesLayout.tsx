import { View, Rescaler, Shader, Text } from '@swmansion/smelter';
import React, { useContext, useEffect, useMemo, useRef, useState } from 'react';
import { useStore } from 'zustand';
import { StoreContext, type InputConfig } from '../store';
import { Input } from '../../inputs/inputs';

// ----- Pure helpers (logic separated from React) -----

const EASING_DURATION_MS = 1200;

function easeInOutCubic(t: number): number {
  return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
}

function computeDesiredIds(inputs: Array<InputConfig>): string[] {
  return inputs.map(i => i.inputId);
}

function buildYOffsetMap(
  ids: string[],
  baseYOffset: number,
  stepYOffset: number
): Record<string, number> {
  const out: Record<string, number> = {};
  for (let idx = 0; idx < ids.length; idx++) {
    out[ids[idx]] = baseYOffset - idx * stepYOffset;
  }
  return out;
}

function xPatternOffset(index: number, stepPx: number): number {
  if (index === 0) {
    return 0;
  }
  const magnitude = Math.ceil(index / 2) * stepPx;
  const sign = index % 2 === 1 ? -1 : 1; // 1:-x, 2:+x, 3:-2x, 4:+2x, ...
  return sign * magnitude;
}

function buildXOffsetMap(ids: string[], xStepPx: number): Record<string, number> {
  const out: Record<string, number> = {};
  for (let idx = 0; idx < ids.length; idx++) {
    out[ids[idx]] = xPatternOffset(idx, xStepPx);
  }
  return out;
}

function buildScaleMap(
  ids: string[],
  baseScale: number,
  shrinkPercent: number
): Record<string, number> {
  const out: Record<string, number> = {};
  const factor = Math.max(0, 1 - shrinkPercent);
  for (let idx = 0; idx < ids.length; idx++) {
    out[ids[idx]] = baseScale * Math.pow(factor, idx);
  }
  return out;
}

function rand01(id: string, salt: number): number {
  let h = 2166136261 >>> 0;
  const s = `${id}:${salt}`;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  const x = Math.sin(h) * 43758.5453;
  return x - Math.floor(x);
}

function buildWobbleMaps(
  ids: string[],
  baseWobbleXAmp: number,
  baseWobbleYAmp: number,
  baseWobbleXFreq: number,
  baseWobbleYFreq: number
): {
  wobbleXAmp: Record<string, number>;
  wobbleYAmp: Record<string, number>;
  wobbleXFreq: Record<string, number>;
  wobbleYFreq: Record<string, number>;
} {
  const wobbleXAmp: Record<string, number> = {};
  const wobbleYAmp: Record<string, number> = {};
  const wobbleXFreq: Record<string, number> = {};
  const wobbleYFreq: Record<string, number> = {};
  for (const id of ids) {
    const rAmpX = rand01(id, 101);
    const rAmpY = rand01(id, 202);
    const rFreqX = rand01(id, 303);
    const rFreqY = rand01(id, 404);
    const ampFactorX = 0.8 + 0.4 * rAmpX; // 0.8..1.2
    const ampFactorY = 0.8 + 0.4 * rAmpY; // 0.8..1.2
    const freqFactorX = 0.7 + 0.6 * rFreqX; // 0.7..1.3
    const freqFactorY = 0.7 + 0.6 * rFreqY; // 0.7..1.3
    wobbleXAmp[id] = baseWobbleXAmp * ampFactorX;
    wobbleYAmp[id] = baseWobbleYAmp * ampFactorY;
    wobbleXFreq[id] = Math.max(0.05, baseWobbleXFreq * freqFactorX);
    wobbleYFreq[id] = Math.max(0.05, baseWobbleYFreq * freqFactorY);
  }
  return { wobbleXAmp, wobbleYAmp, wobbleXFreq, wobbleYFreq };
}

function wrapHue(hue: number): number {
  while (hue > 1) {
    hue -= 1;
  }
  return hue;
}

export function WrappedLayout() {
  const store = useContext(StoreContext);
  const inputs = useStore(store, state => state.inputs);
  if (!inputs.length) {
    return <View />;
  }

  // Compute desired ids based only on inputs to avoid unstable deps
  const desiredIds = useMemo(() => computeDesiredIds(inputs), [inputs]);
  const inputById = useMemo(() => {
    const map: Record<string, (typeof inputs)[number]> = {};
    for (const i of inputs) {
      map[i.inputId] = i;
    }
    return map;
  }, [inputs]);
  // Global (non-animated) shader defaults (local to this layout)
  const shaderDefaults = useMemo(
    () => ({
      circle_diameter: 0.84,
      outline_width: 0.01,
      trail_enable: 1,
      trail_spawn_interval: 0.31,
      trail_speed: 0.53,
      trail_shrink_speed: 0.05,
      trail_x_amplitude: 0.03,
      trail_x_frequency: 2.2,
      trail_count_f32: 10,
      trail_opacity: 0.24,
      wobble_x_amp_px: 25,
      wobble_x_freq: 0.75,
      wobble_y_amp_px: 50,
      wobble_y_freq: 0.5,
    }),
    []
  );

  // Animate per-input Y offsets to their desired positions (based on desired order)
  const baseYOffset = 360;
  const stepYOffset = 100;
  // Horizontal offset pattern params
  const xStepPx = 140; // base X step
  // Scale reduction per subsequent desired index (10% default)
  const shrinkPercent = 0.1;
  const baseCircleScale = 0.22;
  // Base wobble defaults for organic motion
  const baseWobbleXAmp = 25;
  const baseWobbleYAmp = 50;
  const baseWobbleXFreq = 0.75;
  const baseWobbleYFreq = 0.5;

  // Persistent arrival index per input: used to compute offsets/scale based on count at join time
  const [, setArrivalIndexById] = useState<Record<string, number>>({});
  const nextArrivalIndexRef = useRef<number>(0);
  useEffect(() => {
    setArrivalIndexById(prev => {
      const next = { ...prev };
      // Keep monotonic counter in sync with already assigned indices
      const assignedCount = Object.keys(next).length;
      if (nextArrivalIndexRef.current < assignedCount) {
        nextArrivalIndexRef.current = assignedCount;
      }
      // Assign index to new ids in arrival order
      for (const id of desiredIds) {
        if (!(id in next)) {
          next[id] = nextArrivalIndexRef.current++;
        }
      }
      // Drop removed ids (counter stays monotonic)
      for (const id of Object.keys(next)) {
        if (!desiredIds.includes(id)) {
          delete next[id];
        }
      }
      return next;
    });
  }, [desiredIds]);

  // Targets follow desired order (enables swapping animation)
  const targetYOffsetById = useMemo(
    () => buildYOffsetMap(desiredIds, baseYOffset, stepYOffset),
    [desiredIds]
  );
  const targetXOffsetById = useMemo(
    () => buildXOffsetMap(desiredIds, xStepPx),
    [desiredIds, xStepPx]
  );
  const targetScaleById = useMemo(
    () => buildScaleMap(desiredIds, baseCircleScale, shrinkPercent),
    [desiredIds, baseCircleScale, shrinkPercent]
  );
  const {
    wobbleXAmp: targetWobbleXAmpById,
    wobbleYAmp: targetWobbleYAmpById,
    wobbleXFreq: targetWobbleXFreqById,
    wobbleYFreq: targetWobbleYFreqById,
  } = useMemo(
    () =>
      buildWobbleMaps(desiredIds, baseWobbleXAmp, baseWobbleYAmp, baseWobbleXFreq, baseWobbleYFreq),
    [desiredIds]
  );
  // Persistent hue per input: assign once on first sight, keep even if order changes
  const baseOutlineHue = 0.44;
  const hueStep = 0.1;
  const [hueById, setHueById] = useState<Record<string, number>>({});
  // Sequential hue assignment independent of current queue position
  const hueIndexRef = useRef<number>(0);
  useEffect(() => {
    setHueById(prev => {
      const next = { ...prev };
      // Sync counter with how many are already assigned (monotonic, never decremented)
      const assignedCount = Object.keys(next).length;
      if (hueIndexRef.current < assignedCount) {
        hueIndexRef.current = assignedCount;
      }
      // Assign hues to any new ids in arrival order using the sequential counter
      for (const id of desiredIds) {
        if (!(id in next)) {
          next[id] = wrapHue(baseOutlineHue + hueIndexRef.current * hueStep);
          hueIndexRef.current += 1;
        }
      }
      // Drop removed ids (counter stays monotonic)
      for (const id of Object.keys(next)) {
        if (!desiredIds.includes(id)) {
          delete next[id];
        }
      }
      return next;
    });
  }, [desiredIds]);

  const [yOffsetById, setYOffsetById] = useState<Record<string, number>>({});
  const [xOffsetById, setXOffsetById] = useState<Record<string, number>>({});
  const [scaleById, setScaleById] = useState<Record<string, number>>({});
  const animIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const fromRef = useRef<Record<string, number>>({});
  const toRef = useRef<Record<string, number>>({});
  const fromXRef = useRef<Record<string, number>>({});
  const toXRef = useRef<Record<string, number>>({});
  const fromScaleRef = useRef<Record<string, number>>({});
  const toScaleRef = useRef<Record<string, number>>({});

  // Ensure state keys match current inputs: initialize from arrival-based initial maps
  useEffect(() => {
    setYOffsetById(prev => {
      const next = { ...prev };
      const endIdx = Math.max(0, desiredIds.length - 1);
      const initY = baseYOffset - endIdx * stepYOffset;
      for (const id of Object.keys(targetYOffsetById)) {
        if (!(id in next)) {
          next[id] = initY;
        }
      }
      for (const id of Object.keys(next)) {
        if (!(id in targetYOffsetById)) {
          delete next[id];
        }
      }
      return next;
    });
  }, [desiredIds, targetYOffsetById, baseYOffset, stepYOffset]);

  useEffect(() => {
    setXOffsetById(prev => {
      const next = { ...prev };
      const endIdx = Math.max(0, desiredIds.length - 1);
      const initX = xPatternOffset(endIdx, xStepPx);
      for (const id of Object.keys(targetXOffsetById)) {
        if (!(id in next)) {
          next[id] = initX;
        }
      }
      for (const id of Object.keys(next)) {
        if (!(id in targetXOffsetById)) {
          delete next[id];
        }
      }
      return next;
    });
  }, [desiredIds, targetXOffsetById, xStepPx]);

  useEffect(() => {
    setScaleById(prev => {
      const next = { ...prev };
      const endIdx = Math.max(0, desiredIds.length - 1);
      const factor = Math.max(0, 1 - shrinkPercent);
      const initScale = baseCircleScale * Math.pow(factor, endIdx);
      for (const id of Object.keys(targetScaleById)) {
        if (!(id in next)) {
          next[id] = initScale;
        }
      }
      for (const id of Object.keys(next)) {
        if (!(id in targetScaleById)) {
          delete next[id];
        }
      }
      return next;
    });
  }, [desiredIds, targetScaleById, baseCircleScale, shrinkPercent]);

  // Start tween when target offsets change
  useEffect(() => {
    fromRef.current = {};
    toRef.current = {};
    fromXRef.current = {};
    toXRef.current = {};
    fromScaleRef.current = {};
    toScaleRef.current = {};
    let needsAnim = false;
    for (const [id, target] of Object.entries(targetYOffsetById)) {
      const current = yOffsetById[id] ?? target;
      fromRef.current[id] = current;
      toRef.current[id] = target;
      if (Math.abs(current - target) > 0.5) {
        needsAnim = true;
      }
    }
    for (const [id, target] of Object.entries(targetXOffsetById)) {
      const current = xOffsetById[id] ?? target;
      fromXRef.current[id] = current;
      toXRef.current[id] = target;
      if (Math.abs(current - target) > 0.5) {
        needsAnim = true;
      }
    }
    for (const [id, target] of Object.entries(targetScaleById)) {
      const current = scaleById[id] ?? target;
      fromScaleRef.current[id] = current;
      toScaleRef.current[id] = target;
      if (Math.abs(current - target) > 0.001) {
        needsAnim = true;
      }
    }
    if (!needsAnim) {
      return;
    }
    const start = Date.now();
    const tick = () => {
      const t = Math.min(1, (Date.now() - start) / EASING_DURATION_MS);
      const e = easeInOutCubic(t);
      setYOffsetById(prev => {
        const next: Record<string, number> = { ...prev };
        for (const id of Object.keys(toRef.current)) {
          const from = fromRef.current[id] ?? toRef.current[id];
          const to = toRef.current[id];
          next[id] = from + (to - from) * e;
        }
        return next;
      });
      setXOffsetById(prev => {
        const next: Record<string, number> = { ...prev };
        for (const id of Object.keys(toXRef.current)) {
          const from = fromXRef.current[id] ?? toXRef.current[id];
          const to = toXRef.current[id];
          next[id] = from + (to - from) * e;
        }
        return next;
      });
      setScaleById(prev => {
        const next: Record<string, number> = { ...prev };
        for (const id of Object.keys(toScaleRef.current)) {
          const from = fromScaleRef.current[id] ?? toScaleRef.current[id];
          const to = toScaleRef.current[id];
          next[id] = from + (to - from) * e;
        }
        return next;
      });
      if (t >= 1 && animIntervalRef.current) {
        clearInterval(animIntervalRef.current);
        animIntervalRef.current = null;
      }
    };
    if (animIntervalRef.current) {
      clearInterval(animIntervalRef.current);
      animIntervalRef.current = null;
    }
    animIntervalRef.current = setInterval(tick, 16);
    return () => {
      if (animIntervalRef.current) {
        clearInterval(animIntervalRef.current);
        animIntervalRef.current = null;
      }
    };
  }, [targetYOffsetById, targetXOffsetById, targetScaleById]);

  return (
    <View style={{ direction: 'column', width: 2560, height: 1440 }}>
      <Rescaler
        style={{
          rescaleMode: 'fill',
          horizontalAlign: 'left',
          verticalAlign: 'top',
          width: 2560,
          height: 1440,
          top: 0,
          left: 0,
        }}>
        <Shader
          shaderId="star-streaks"
          resolution={{ width: 2560, height: 1440 }}
          shaderParam={{
            type: 'struct',
            value: [
              { type: 'f32', fieldName: 'line_density', value: 18.91 },
              { type: 'f32', fieldName: 'thickness_px', value: 2.0 },
              { type: 'f32', fieldName: 'speed', value: 2.45 },
              { type: 'f32', fieldName: 'jitter_amp_px', value: 48.0 },
              { type: 'f32', fieldName: 'jitter_freq', value: 0.15 },
              { type: 'f32', fieldName: 'dash_repeat', value: 2.0 },
              { type: 'f32', fieldName: 'dash_duty', value: 0.19 },
              { type: 'f32', fieldName: 'brightness', value: 0.26 },
            ],
          }}>
          <View
            style={{ width: 2560, height: 1440, backgroundColor: '#000000', direction: 'column' }}
          />
        </Shader>
      </Rescaler>

      {desiredIds.map((id, renderIdx) => {
        const input = inputById[id];
        if (!input) {
          return null;
        }
        // Prefer animated state; fallback to targets (dependent on current order)
        const yOffset =
          (id in yOffsetById ? yOffsetById[id] : targetYOffsetById[id]) ??
          baseYOffset - renderIdx * stepYOffset;
        const xOffset = (id in xOffsetById ? xOffsetById[id] : targetXOffsetById[id]) ?? 0;
        const circleScale =
          (id in scaleById ? scaleById[id] : targetScaleById[id]) ?? baseCircleScale;
        return (
          <Rescaler
            key={input.inputId}
            style={{
              rescaleMode: 'fill',
              horizontalAlign: 'left',
              verticalAlign: 'top',
              width: Math.round(2560),
              height: Math.round(1440),
              top: 0,
              left: 0,
            }}>
            <Shader
              shaderId="circle-mask-outline"
              resolution={{ width: 1920, height: 1080 }}
              shaderParam={{
                type: 'struct',
                value: [
                  // Global, user-adjustable defaults (non-animated)
                  {
                    type: 'f32',
                    fieldName: 'circle_diameter',
                    value: shaderDefaults.circle_diameter,
                  },
                  { type: 'f32', fieldName: 'outline_width', value: shaderDefaults.outline_width },
                  { type: 'f32', fieldName: 'outline_hue', value: hueById[id] ?? 0.44 },
                  { type: 'f32', fieldName: 'circle_scale', value: circleScale },
                  { type: 'f32', fieldName: 'circle_offset_x_px', value: xOffset },
                  // Animated per-input vertical offset
                  { type: 'f32', fieldName: 'circle_offset_y_px', value: yOffset },
                  // Free oscillation (organic per input)
                  {
                    type: 'f32',
                    fieldName: 'wobble_x_amp_px',
                    value: targetWobbleXAmpById[id] ?? baseWobbleXAmp,
                  },
                  {
                    type: 'f32',
                    fieldName: 'wobble_x_freq',
                    value: targetWobbleXFreqById[id] ?? baseWobbleXFreq,
                  },
                  {
                    type: 'f32',
                    fieldName: 'wobble_y_amp_px',
                    value: targetWobbleYAmpById[id] ?? baseWobbleYAmp,
                  },
                  {
                    type: 'f32',
                    fieldName: 'wobble_y_freq',
                    value: targetWobbleYFreqById[id] ?? baseWobbleYFreq,
                  },
                  // Trail defaults
                  { type: 'f32', fieldName: 'trail_enable', value: shaderDefaults.trail_enable },
                  {
                    type: 'f32',
                    fieldName: 'trail_spawn_interval',
                    value: shaderDefaults.trail_spawn_interval,
                  },
                  { type: 'f32', fieldName: 'trail_speed', value: shaderDefaults.trail_speed },
                  {
                    type: 'f32',
                    fieldName: 'trail_shrink_speed',
                    value: shaderDefaults.trail_shrink_speed,
                  },
                  {
                    type: 'f32',
                    fieldName: 'trail_x_amplitude',
                    value: shaderDefaults.trail_x_amplitude,
                  },
                  {
                    type: 'f32',
                    fieldName: 'trail_x_frequency',
                    value: shaderDefaults.trail_x_frequency,
                  },
                  {
                    type: 'f32',
                    fieldName: 'trail_count_f32',
                    value: shaderDefaults.trail_count_f32,
                  },
                  { type: 'f32', fieldName: 'trail_opacity', value: shaderDefaults.trail_opacity },
                ],
              }}>
              <View
                style={{
                  direction: 'column',
                  overflow: 'visible',
                  top: 0,
                  left: 0,
                  width: 1920,
                  height: 1080,
                }}>
                <Input input={input} />
                <Text style={{ fontSize: 80, color: '#ffffff' }}>420</Text>
              </View>
            </Shader>
          </Rescaler>
        );
      })}
    </View>
  );
}
