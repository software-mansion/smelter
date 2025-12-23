import { View, Tiles, Rescaler, Image, Text } from '@swmansion/smelter';
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
        const minLeft = -3120;
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
          top: 960,
          left: 0,
        }}>
        <NewsStripDecorated
          resolution={{ width: 2560, height: 450 }}
          opacity={1}
          amplitudePx={waveAmpPx}
          wavelengthPx={800}
          speed={waveSpeed}
          phase={0}
          removeColorTolerance={0.4}>
          <View style={{ width: 2560, height: 450, direction: 'column' }}>
            {/* left logo box */}
            <View
              style={{
                width: 240,
                height: 72,
                top: 114,
                left: 0,
                direction: 'column',
                overflow: 'hidden',
                backgroundColor: '#F24664',
              }}>
              <Text
                style={{
                  fontSize: 40,
                  lineHeight: 72,
                  color: '#000000',
                  fontFamily: 'Poppins',
                  fontWeight: 'bold',
                  align: 'center',
                  width: 240,
                  height: 72,
                }}>
                LIVE
              </Text>
            </View>
            <View
              style={{
                width: 240,
                height: 192,
                top: Math.round((450 - 80) / 2),
                left: 0,
                direction: 'column',
                overflow: 'hidden',
                backgroundColor: '#ffffff',
              }}>
              <Rescaler style={{ rescaleMode: 'fill', width: 150, height: 72, top: 56, left: 50 }}>
                <Image imageId="smelter_logo" />
              </Rescaler>
            </View>
            <View
              style={{
                width: 2320,
                height: 192,
                top: Math.round((450 - 80) / 2),
                left: 240,
                direction: 'column',
                overflow: 'hidden',
                backgroundColor: '#342956',
              }}>
              <View
                style={{
                  direction: 'column',
                  height: 192,
                  width: 3560,
                  overflow: 'visible',
                  padding: 10,
                  top: 48,
                  left: Math.round(marqueeLeft),
                }}>
                <Text
                  style={{
                    fontSize: 90,
                    width: 3560,
                    color: '#ffffff',
                    fontFamily: 'Poppins',
                    fontWeight: 'semi_bold',
                  }}>
                  {'This video is composing multiple live streams in real time using smelter.'.toUpperCase()}
                </Text>
              </View>
            </View>
          </View>
        </NewsStripDecorated>
      </Rescaler>
    </View>
  );
}
