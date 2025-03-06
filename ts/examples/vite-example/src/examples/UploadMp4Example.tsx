import { useEffect, useState } from 'react';
import Smelter from '@swmansion/smelter-web-wasm';
import { InputStream, Rescaler, View } from '@swmansion/smelter';
import CompositorVideo from '../components/SmelterVideo';

// TODO(noituri): Make the upload button nicer and the whole example nicer

function UploadMp4Example() {
  const smelter = useSmelter();
  const onUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (!e.target.files) {
      console.error("no files");
      return;
    }

    let file = e.target.files[0];
    console.log(file);

    if (!smelter) {
      console.error("no smelter");
      return;
    }

    smelter.registerInput('file', { type: 'mp4', blob: file });
  }

  if (!smelter) {
    return <div className="card" />;
  }

  return (
    <div className="card">
      <input type='file' onChange={onUpload} />
      <CompositorVideo outputId='output' width={1280} height={720} smelter={smelter}>
        <Scene />
      </CompositorVideo>
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
        await promise.catch(() => { });
        await smelter.terminate();
      })();
    };
  }, []);
  return smelter;
}

export default UploadMp4Example;
