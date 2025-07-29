import { Shader, View, Rescaler } from '@swmansion/smelter';
import { Chat } from './overlays/chat';
import { InputStream } from '@swmansion/smelter';
import { ChatContextProvider } from './overlays/chat/context';

export default function App() {
  return <OutputScene />;
}

function OutputScene() {
  return (
    <View style={{ width: 1920, height: 1080 }}>
      <ChatContextProvider>
        <Chat />
      </ChatContextProvider>
      <Rescaler>
        <Shader
          shaderId="ascii-filter"
          resolution={{ width: 1920, height: 1080 }}
          shaderParam={{
            type: 'struct',
            value: [
              {
                type: 'f32',
                fieldName: 'glyph_size',
                value: 32,
              },
              {
                type: 'f32',
                fieldName: 'gamma_correction',
                value: 0.3,
              },
            ],
          }}>
          <InputStream inputId="bunny" />
        </Shader>
      </Rescaler>
    </View>
  );
}
