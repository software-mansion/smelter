import { View, Rescaler } from '@swmansion/smelter';
import React, { useContext } from 'react';
import { useStore } from 'zustand';
import { StoreContext } from '../store';
import { Input } from '../../inputs/inputs';

export function MultiplePicturesLayout() {
  const store = useContext(StoreContext);
  const inputs = useStore(store, state => state.inputs);
  if (!inputs.length) {
    return <View />;
  }

  // Arrange around the main (first) input: alternately to the right and left.
  const main = inputs[0];
  const others = inputs.slice(1);
  const right: typeof others = []; // 2,4,6...
  const left: typeof others = []; // 3,5,7...
  for (let i = 0; i < others.length; i++) {
    if (i % 2 === 0) {
      right.push(others[i]);
    } else {
      left.push(others[i]);
    }
  }
  // Draw order: far left (bottom) -> near left -> main -> near right -> far right (top)
  const ordered = [...left.slice().reverse(), main, ...right];

  // Horizontal offsets so layers peek alternately from center
  const offsetStep = 300; // px
  // Vertical offsets so each subsequent layer is a bit higher
  const offsetStepY = 120; // px
  // Scale step for each layer away from the main input
  const scaleStep = 0.15;

  const getOffsetLeft = (inputId: string) => {
    if (inputId === main.inputId) {
      return 0;
    }
    const leftIndex = left.findIndex(i => i.inputId === inputId);
    if (leftIndex !== -1) {
      return -(leftIndex + 1) * offsetStep;
    }
    const rightIndex = right.findIndex(i => i.inputId === inputId);
    if (rightIndex !== -1) {
      return (rightIndex + 1) * offsetStep;
    }
    return 0;
  };

  const getScale = (inputId: string) => {
    if (inputId === main.inputId) {
      return 1;
    }
    const leftIndex = left.findIndex(i => i.inputId === inputId);
    if (leftIndex !== -1) {
      const k = leftIndex + 1;
      return Math.max(0.2, 1 - k * scaleStep);
    }
    const rightIndex = right.findIndex(i => i.inputId === inputId);
    if (rightIndex !== -1) {
      const k = rightIndex + 1;
      return Math.max(0.2, 1 - k * scaleStep);
    }
    return 1;
  };

  const getOffsetTop = (inputId: string) => {
    if (inputId === main.inputId) {
      return 0;
    }
    const leftIndex = left.findIndex(i => i.inputId === inputId);
    if (leftIndex !== -1) {
      const k = leftIndex + 1;
      return -(k * offsetStepY);
    }
    const rightIndex = right.findIndex(i => i.inputId === inputId);
    if (rightIndex !== -1) {
      const k = rightIndex + 1;
      return -(k * offsetStepY);
    }
    return 0;
  };

  return (
    <View style={{ direction: 'column', width: 2560, height: 1440 }}>
      {ordered.map(input => (
        <Rescaler
          key={input.inputId}
          style={{
            rescaleMode: 'fill',
            horizontalAlign: 'left',
            verticalAlign: 'top',
            width: Math.round(2560 * getScale(input.inputId)),
            height: Math.round(1440 * getScale(input.inputId)),
            top: getOffsetTop(input.inputId),
            left: getOffsetLeft(input.inputId),
          }}>
          <Input input={input} />
        </Rescaler>
      ))}
    </View>
  );
}
