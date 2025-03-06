import React, { useCallback, useEffect, useState } from 'react';
import Smelter from '@swmansion/smelter-web-wasm';
import { InputStream, Rescaler, useInputStreams, View, Text } from '@swmansion/smelter';
import CompositorVideo from '../components/SmelterVideo';
import NotoSansFont from '../../assets/NotoSans.ttf';

function UploadMp4Example() {
  const smelter = useSmelter();

  const onCreate = useCallback(async (smelter: Smelter) => {
    await smelter.registerFont(NotoSansFont);
  }, []);
  const onUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    if (!e.target.files) {
      console.error('No files were uploaded');
      return;
    }

    let file = e.target.files[0];

    if (!smelter) {
      console.error('Smelter has not been initialized yet');
      return;
    }

    await smelter.unregisterInput('file');
    await smelter.registerInput('file', { type: 'mp4', blob: file });
  };

  if (!smelter) {
    return <div className="card" />;
  }

  return (
    <div className="card">
      <div style={{ margin: 20 }}>
        <label htmlFor="upload-input" style={{ padding: 10 }}>
          Upload MP4
        </label>
        <input id="upload-input" type="file" onChange={onUpload} accept="video/mp4" />
      </div>
      <CompositorVideo
        outputId="output"
        width={1280}
        height={720}
        smelter={smelter}
        onVideoCreated={onCreate}>
        <Scene />
      </CompositorVideo>
    </div>
  );
}

function Scene() {
  const inputs = useInputStreams();
  if (!inputs['file']) {
    return (
      <View style={{ backgroundColor: 'black' }}>
        <View style={{ top: 340, left: 560 }}>
          <Text style={{ fontSize: 24, color: 'white' }}>Upload an MP4 file</Text>
        </View>
      </View>
    );
  }
  return (
    <View style={{ backgroundColor: 'black' }}>
      <Rescaler>
        <InputStream inputId="file" />
      </Rescaler>
    </View>
  );
}

function useSmelter(): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  useEffect(() => {
    const smelter = new Smelter();

    let cancel = false;
    const promise = (async () => {
      await smelter.init();
      await smelter.start();
      if (!cancel) {
        setSmelter(smelter);
      }
    })();

    return () => {
      cancel = true;
      void (async () => {
        await promise.catch(() => {});
        await smelter.terminate();
      })();
    };
  }, []);
  return smelter;
}

export default UploadMp4Example;
