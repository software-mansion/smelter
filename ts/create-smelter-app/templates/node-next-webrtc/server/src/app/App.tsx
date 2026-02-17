import { View, InputStream, Rescaler, Text } from '@swmansion/smelter';
import { useStore } from 'zustand';
import { store } from './store';

export default function App() {
  const showInstructions = useStore(store, state => state.showInstructions);

  return (
    <View>
      <Rescaler style={{ rescaleMode: 'fill' }}>
        <InputStream inputId="input" />
      </Rescaler>
      <View
        style={{
          // Negative value will move this element of screen. We could wrap
          // element with a conditional logic, but then it would be removed
          // immediately without transition.
          right: showInstructions ? 40 : -660,
          top: 40,
          width: 580,
          height: 920,
          padding: 40,
          backgroundColor: '#FFFFFF55',
          borderRadius: 20,
          direction: 'column',
        }}
        transition={{ durationMs: 300 }}>
        <Text style={{ fontSize: 30, width: 500, wrap: 'word', color: '#000000' }}>
          - Send your webcam or screen share to the smelter.
        </Text>
        <Text style={{ fontSize: 30, width: 500, wrap: 'word', color: '#000000' }}>
          - Uncheck "Show instructions" checkbox to hide this component.
        </Text>
        <View style={{ height: 40 }} />
        <Text style={{ fontSize: 30, width: 500, wrap: 'word', color: '#000000' }}>
          Open /server/src/app/App.tsx to modify layout of the video.
        </Text>
      </View>
    </View>
  );
}
