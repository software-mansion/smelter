import { View, Rescaler, Tiles } from '@swmansion/smelter';
import React, { useContext } from 'react';
import { useStore } from 'zustand';
import { StoreContext } from '../store';
import { Input, SmallInput } from '../../inputs/inputs';

export function PrimaryOnTopLayout() {
  const store = useContext(StoreContext);
  const inputs = useStore(store, state => state.inputs);
  const firstInput = inputs[0];
  if (!firstInput) {
    return <View />;
  }

  return (
    <View style={{ direction: 'column' }}>
      <Rescaler style={{ height: 800 }}>
        <Input input={firstInput} />
      </Rescaler>
      <Tiles transition={{ durationMs: 300 }} style={{ padding: 10 }}>
        {Object.values(inputs)
          .filter(input => input.inputId != firstInput.inputId)
          .map(input => (
            <SmallInput key={input.inputId} input={input} />
          ))}
      </Tiles>
    </View>
  );
}
