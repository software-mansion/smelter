import { View, Text, InputStream } from '@swmansion/smelter';
import { useEffect, useState } from 'react';

export function Playback() {
  const [counter, setCounter] = useState<number>(0);

  useEffect(() => {
    const id = setInterval(() => setCounter(old => old + 1), 1000);
    return () => clearInterval(id);
  }, []);

  return (
    <View>
      <View style={{ left: 25, top: 25 }}>
        <Text>{counter}</Text>
      </View>
      <InputStream inputId="input" />
    </View>
  );
}
