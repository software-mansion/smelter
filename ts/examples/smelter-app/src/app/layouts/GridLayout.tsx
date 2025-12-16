import { Tiles } from '@swmansion/smelter';
import React, { useContext } from 'react';
import { useStore } from 'zustand';
import { StoreContext } from '../store';
import { Input } from '../../inputs/inputs';

export function GridLayout() {
  const store = useContext(StoreContext);
  const inputs = useStore(store, state => state.inputs);

  return (
    <Tiles transition={{ durationMs: 300 }} style={{ padding: 20, tileAspectRatio: '1920:1210' }}>
      {Object.values(inputs).map(input => (
        <Input key={input.inputId} input={input} />
      ))}
    </Tiles>
  );
}
