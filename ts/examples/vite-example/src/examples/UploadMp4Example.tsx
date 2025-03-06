import { useCallback, useRef } from 'react';
import Smelter from '@swmansion/smelter-web-wasm';
import { InputStream, Rescaler, View } from '@swmansion/smelter';
import CompositorCanvas from '../components/SmelterCanvas';

function UploadMp4Example() {
  const uploadRef = useRef<HTMLInputElement | null>(null);
  const onCanvasCreate = useCallback(async (compositor: Smelter) => {
    uploadRef.current!.onchange = (e: Event): any => {
      const file = (e.target as HTMLInputElement).files?.[0];
      if (!file) {
        console.error("no file");
        return;
      }
      console.log(file);
      compositor.registerInput('file', { type: 'mp4', blob: file });
    };
    try {
    } catch (err: any) {
      console.warn('Failed to register camera input', err);
    }
  }, []);

  return (
    <div className="card">
      <input type='file' ref={uploadRef} />
      <CompositorCanvas onCanvasCreated={onCanvasCreate} width={1280} height={720}>
        <Scene />
      </CompositorCanvas>
    </div>
  );
}

function Scene() {
  return (
    <View>
      <Rescaler>
        <InputStream inputId="file" />
      </Rescaler>
    </View>
  );
}

export default UploadMp4Example;
