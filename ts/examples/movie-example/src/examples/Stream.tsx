import { InputStream, Mp4, Rescaler, useInputStreams, View } from '@swmansion/smelter';
import { useStore } from 'zustand';
import { store } from '../store';
import CommercialMp4 from '../../assets/appjs.mp4';

function Stream() {
  const showCommercial = useStore(store, state => state.showCommercial);

  const inputs = useInputStreams();
  const hasScreenCapture = !!inputs['screen'];
  const cameraPosition = hasScreenCapture
    ? {
        top: 16,
        left: 16,
        width: 256,
        height: 180,
      }
    : {
        top: 1,
        left: 1,
        width: 1024,
        height: 576,
      };

  if (showCommercial) {
    return (
      <Rescaler
        style={{
          borderRadius: 24,
          borderColor: 'white',
          borderWidth: 1,
        }}>
        <Mp4 source={new URL(CommercialMp4, import.meta.url).toString()} />
      </Rescaler>
    );
  }

  return (
    <View style={{ backgroundColor: '#161127' }}>
      <View
        style={{
          borderRadius: 24,
          borderColor: 'white',
          borderWidth: 1,
          padding: 24,
          backgroundColor: '#424242',
        }}>
        <Rescaler style={{ horizontalAlign: 'right' }}>
          <InputStream inputId="screen" />
        </Rescaler>
      </View>
      <Rescaler
        transition={{ durationMs: 650 }}
        style={{
          ...cameraPosition,
          rescaleMode: 'fill',
          borderRadius: 24,
        }}>
        <InputStream inputId="camera" />
      </Rescaler>
    </View>
  );
}

export default Stream;
