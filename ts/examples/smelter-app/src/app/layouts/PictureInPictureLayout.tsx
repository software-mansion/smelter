import { View, Tiles, Rescaler, Image, Text, Shader } from '@swmansion/smelter';
import React, { useContext, useEffect, useState } from 'react';
import { useStore } from 'zustand';
import { StoreContext } from '../store';
import { Input, SmallInput } from '../../inputs/inputs';
import { NewsStripDecorated } from '../NewsStripDecorated';

export function PictureInPictureLayout() {
  const store = useContext(StoreContext);
  const inputs = useStore(store, state => state.inputs);
  const firstInput = inputs[0];
  const secondInput = inputs[1];

  if (!firstInput) {
    return <View />;
  }

  const [waveAmpPx, setWaveAmpPx] = useState(0);
  const [waveSpeed, setWaveSpeed] = useState(0);
  const [marqueeLeft, setMarqueeLeft] = useState(2560);
  useEffect(() => {
    let mounted = true;
    let tweenId: ReturnType<typeof setInterval> | null = null;
    let timerId: ReturnType<typeof setTimeout> | null = null;
    let marqueeId: ReturnType<typeof setInterval> | null = null;
    const tween = (from: number, to: number, ms: number) => {
      if (tweenId) {
        clearInterval(tweenId);
        tweenId = null;
      }
      const start = Date.now();
      tweenId = setInterval(() => {
        const t = Math.min(1, (Date.now() - start) / Math.max(1, ms));
        const val = from + (to - from) * t;
        if (!mounted) {
          return;
        }
        setWaveAmpPx(Math.max(0, val));
        if (t >= 1) {
          if (tweenId) {
            clearInterval(tweenId);
            tweenId = null;
          }
        }
      }, 16);
    };
    const runCycle = () => {
      if (!mounted) {
        return;
      }
      setWaveSpeed(0);
      setWaveAmpPx(0);
      if (!marqueeId) {
        const pxPerSec = 240;
        const intervalMs = 10;
        const step = (pxPerSec * intervalMs) / 1000;
        const resetRight = 2560;
        const minLeft = -2120;
        marqueeId = setInterval(() => {
          if (!mounted) {
            return;
          }
          setMarqueeLeft(prev => {
            const next = prev - step;
            return next < minLeft ? resetRight : next;
          });
        }, intervalMs);
      }
      timerId = setTimeout(() => {
        if (!mounted) {
          return;
        }
        setWaveSpeed(6);
        tween(0, 25, 500);
        timerId = setTimeout(() => {
          if (!mounted) {
            return;
          }
          tween(25, 0, 500);
          timerId = setTimeout(() => {
            if (!mounted) {
              return;
            }
            runCycle();
          }, 4000);
        }, 2000);
      }, 3000);
    };
    runCycle();
    return () => {
      mounted = false;
      if (tweenId) {
        clearInterval(tweenId);
      }
      if (timerId) {
        clearTimeout(timerId);
      }
    };
  }, []);

  return (
    <View style={{ direction: 'column' }}>
      <Rescaler
        transition={{ durationMs: 300 }}
        style={{
          rescaleMode: 'fill',
          horizontalAlign: 'left',
          verticalAlign: 'top',
          width: 2560,
          height: 1440,
          top: 0,
          left: 0,
        }}>
        <Input input={firstInput} />
      </Rescaler>
      {secondInput ? (
        <Rescaler style={{ top: 60, right: 60, width: 640, height: 1080 }}>
          <View style={{ direction: 'column' }}>
            <Tiles transition={{ durationMs: 300 }} style={{ padding: 10, verticalAlign: 'top' }}>
              {Object.values(inputs)
                .filter(input => input.inputId != firstInput.inputId)
                .map(input => (
                  <SmallInput key={input.inputId} input={input} />
                ))}
            </Tiles>
          </View>
        </Rescaler>
      ) : null}
      <Rescaler
        transition={{ durationMs: 300 }}
        style={{
          rescaleMode: 'fill',
          horizontalAlign: 'left',
          verticalAlign: 'top',
          width: 2560,
          height: 450,
          top: 1000,
          left: 0,
        }}>
        <NewsStripDecorated
          resolution={{ width: 2560, height: 450 }}
          opacity={0.8}
          amplitudePx={waveAmpPx}
          wavelengthPx={800}
          speed={waveSpeed}
          phase={0}
          removeColorTolerance={0.4}>
          <View style={{ width: 2560, height: 450, direction: 'column' }}>
            {
              <Rescaler style={{ rescaleMode: 'fill', width: 2560, height: 450 }}>
                <Image imageId="news_strip" />
              </Rescaler>
            }

            <View
              style={{
                width: 2560,
                height: 80,
                top: Math.round((450 - 80) / 2),
                left: 0,
                direction: 'column',
                overflow: 'visible',
              }}>
              <View
                style={{
                  direction: 'column',
                  height: 80,
                  padding: 10,
                  top: 0,
                  left: Math.round(marqueeLeft),
                }}>
                <Shader
                  shaderId="soft-shadow"
                  resolution={{ width: 1920, height: 80 }}
                  shaderParam={{
                    type: 'struct',
                    value: [
                      { type: 'f32', fieldName: 'shadow_r', value: 0 },
                      { type: 'f32', fieldName: 'shadow_g', value: 0 },
                      { type: 'f32', fieldName: 'shadow_b', value: 0 },
                      { type: 'f32', fieldName: 'opacity', value: 0.6 },
                      { type: 'f32', fieldName: 'offset_x_px', value: -12 },
                      { type: 'f32', fieldName: 'offset_y_px', value: -12 },
                      { type: 'f32', fieldName: 'blur_px', value: 6 },
                      { type: 'f32', fieldName: 'anim_amp_px', value: 1.5 },
                      { type: 'f32', fieldName: 'anim_speed', value: 2 },
                    ],
                  }}>
                  <Text
                    style={{
                      fontSize: 80,
                      color: '#7ecbff',
                      fontFamily: 'Roboto Sans',
                    }}>
                    This video is composing multiple live streams in real-time by Smelter.{' '}
                  </Text>
                </Shader>
              </View>
            </View>
          </View>
        </NewsStripDecorated>
      </Rescaler>
    </View>
  );
}
