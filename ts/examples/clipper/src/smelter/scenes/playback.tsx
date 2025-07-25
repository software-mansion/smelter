import { View, Text } from '@swmansion/smelter';
import { useEffect, useState } from 'react';

export function Playback() {
  const [counter, setCounter] = useState<number>(0);

  useEffect(() => {
    const id = setInterval(() => setCounter(old => old + 1), 1000);
    return () => clearInterval(id);
  }, []);

  return (
    <View>
      <Text>{counter}</Text>
    </View>
  );
}
