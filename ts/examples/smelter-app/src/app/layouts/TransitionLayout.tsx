import { View, Rescaler, Shader } from '@swmansion/smelter';
import React, { useContext, useEffect, useRef, useState } from 'react';
import { useStore } from 'zustand';
import { StoreContext } from '../store';
import { Input } from '../../inputs/inputs';

export function TransitionLayout() {
  const store = useContext(StoreContext);
  const inputs = useStore(store, state => state.inputs);
  const inputA = inputs[0];
  const inputB = inputs[1];

  const speedDefault = 0.25;
  const pauseSeconds = 3;
  const intervalMs = 10;

  const [progress, setProgress] = useState(0);
  const directionRef = useRef(1);
  const speedRef = useRef(speedDefault);

  useEffect(() => {
    let timer: ReturnType<typeof setInterval> | null = null;
    let pauseTimer: ReturnType<typeof setTimeout> | null = null;

    function startAnimation() {
      timer = setInterval(() => {
        setProgress(prev => {
          let next = prev + directionRef.current * speedRef.current * (intervalMs / 1000);
          if (directionRef.current === 1 && next >= 1) {
            next = 1;
            clearInterval(timer!);
            timer = null;
            pauseTimer = setTimeout(() => {
              directionRef.current = -1;
              startAnimation();
            }, pauseSeconds * 1000);
          } else if (directionRef.current === -1 && next <= 0) {
            next = 0;
            clearInterval(timer!);
            timer = null;
            pauseTimer = setTimeout(() => {
              directionRef.current = 1;
              startAnimation();
            }, pauseSeconds * 1000);
          }
          return Math.max(0, Math.min(1, next));
        });
      }, intervalMs);
    }
    startAnimation();

    return () => {
      if (timer) {
        clearInterval(timer);
      }
      if (pauseTimer) {
        clearTimeout(pauseTimer);
      }
    };
  }, []);

  if (!inputA) {
    return <View />;
  }

  const resolution = { width: 1920, height: 1080 };

  let showFirst, showSecond;
  if (progress < 0.5) {
    showFirst = inputA;
    showSecond = inputB;
  } else {
    showFirst = inputB;
    showSecond = inputA;
  }

  if (!showFirst) {
    return <View />;
  }

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
          shaderId="page-flip-1"
          resolution={resolution}
          shaderParam={{
            type: 'struct',
            value: [
              { type: 'f32', fieldName: 'progress', value: progress },
              { type: 'f32', fieldName: 'direction', value: 0 },
              { type: 'f32', fieldName: 'perspective', value: 1 },
              { type: 'f32', fieldName: 'shadow_strength', value: 0.75 },
              { type: 'f32', fieldName: 'back_tint', value: 0.45 },
              { type: 'f32', fieldName: 'back_tint_strength', value: 0.33 },
            ],
          }}>
          {showFirst ? <Input input={showFirst} /> : <View />}
        </Shader>
      </Rescaler>
      {showSecond ? (
        <Rescaler
          style={{
            rescaleMode: 'fill',
            horizontalAlign: 'left',
            verticalAlign: 'top',
            top: 0,
            left: 0,
          }}>
          <Shader
            shaderId="page-flip-1"
            resolution={resolution}
            shaderParam={{
              type: 'struct',
              value: [
                { type: 'f32', fieldName: 'progress', value: progress },
                { type: 'f32', fieldName: 'direction', value: 0 },
                { type: 'f32', fieldName: 'perspective', value: 1 },
                { type: 'f32', fieldName: 'shadow_strength', value: 0.75 },
                { type: 'f32', fieldName: 'back_tint', value: 0.45 },
                { type: 'f32', fieldName: 'back_tint_strength', value: 0.33 },
              ],
            }}>
            {showSecond ? <Input input={showSecond} /> : <View />}
          </Shader>
        </Rescaler>
      ) : null}
    </View>
  );
}
