import React, { useCallback, useEffect, useState } from 'react';
import Smelter from '@swmansion/smelter-web-wasm';
import { InputStream, Rescaler, Text, Tiles, useInputStreams, View } from '@swmansion/smelter';
import NotoSansFont from '../../assets/NotoSans.ttf';

const FIRST_MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerEscapes.mp4';

const SECOND_MP4_URL =
  'https://commondatastorage.googleapis.com/gtv-videos-bucket/sample/ForBiggerBlazes.mp4';

function MultipleOutputs() {
  const smelter = useSmelter();

  useEffect(() => {
    if (!smelter) {
      return;
    }
    void (async () => {
      await smelter.registerFont(NotoSansFont);
      await smelter.registerInput('input_1', { type: 'mp4', url: FIRST_MP4_URL });
      await new Promise<void>(res => setTimeout(() => res(), 3000));
      await smelter.registerInput('input_2', { type: 'mp4', url: SECOND_MP4_URL });
    })();
  }, [smelter]);

  if (!smelter) {
    return <div className="card" />;
  }

  return (
    <div className="card">
      <h2>Inputs</h2>
      <div style={{ flexDirection: 'row', display: 'flex' }}>
        <CompositorVideo
          style={{ margin: 20 }}
          outputId="input1_preview"
          width={600}
          height={340}
          smelter={smelter}>
          <Rescaler style={{ borderWidth: 5, borderColor: 'white', rescaleMode: 'fill' }}>
            <InputStream inputId="input_1" muted={true} />
          </Rescaler>
        </CompositorVideo>
        <CompositorVideo
          style={{ margin: 20 }}
          outputId="input2_preview"
          width={600}
          height={340}
          smelter={smelter}>
          <Rescaler style={{ borderWidth: 5, borderColor: 'white', rescaleMode: 'fill' }}>
            <InputStream inputId="input_2" muted={true} />
          </Rescaler>
        </CompositorVideo>
      </div>

      <h2>Outputs</h2>
      <CompositorVideo
        style={{ margin: 20 }}
        outputId="output"
        width={1280}
        height={720}
        smelter={smelter}>
        <Scene />
      </CompositorVideo>
    </div>
  );
}

function SceneTile(props: { state?: 'ready' | 'playing' | 'finished'; inputId: string }) {
  if (props.state === 'playing') {
    return (
      <View>
        <Rescaler style={{ rescaleMode: 'fill' }}>
          <InputStream inputId={props.inputId} />
        </Rescaler>
        <View style={{ width: 230, height: 40, bottom: 20, left: 20 }}>
          <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Playing MP4 file</Text>
        </View>
      </View>
    );
  }

  if (props.state === 'finished') {
    return (
      <View style={{ backgroundColor: '#000000' }}>
        <View style={{ width: 530, height: 40, bottom: 20, left: 20 }}>
          <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Finished playing MP4 file</Text>
        </View>
      </View>
    );
  }

  return (
    <View style={{ backgroundColor: '#000000' }}>
      <View style={{ width: 530, height: 40, bottom: 20, left: 20 }}>
        <Text style={{ fontSize: 30, fontFamily: 'Noto Sans' }}>Loading MP4 file</Text>
      </View>
    </View>
  );
}

function Scene() {
  const inputs = useInputStreams();
  return (
    <View style={{ borderWidth: 5, borderColor: 'white', backgroundColor: 'black' }}>
      <Tiles transition={{ durationMs: 500 }}>
        {Object.values(inputs).map(input => (
          <SceneTile key={input.inputId} state={input.videoState} inputId={input.inputId} />
        ))}
      </Tiles>
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

type VideoProps = React.DetailedHTMLProps<
  React.VideoHTMLAttributes<HTMLVideoElement>,
  HTMLVideoElement
>;

type CompositorVideoProps = {
  outputId: string;
  onVideoCreated?: (smelter: Smelter) => Promise<void>;
  smelter: Smelter;
  children: React.ReactElement;
} & VideoProps;

function CompositorVideo(props: CompositorVideoProps) {
  const { outputId, onVideoCreated, children, smelter: initialSmelter, ...videoProps } = props;
  const [smelter, _setSmelter] = useState<Smelter>(initialSmelter);

  const videoRef = useCallback(
    async (video: HTMLVideoElement | null) => {
      if (!video) {
        return;
      }

      if (onVideoCreated) {
        await onVideoCreated(smelter);
      }

      const { stream } = await smelter.registerOutput(outputId, children, {
        type: 'stream',
        video: {
          resolution: {
            width: Number(videoProps.width ?? video.width),
            height: Number(videoProps.height ?? video.height),
          },
        },
        audio: true,
      });

      if (stream) {
        video.srcObject = stream;
        await video.play();
      }
    },
    [onVideoCreated, videoProps.width, videoProps.height, smelter, outputId]
  );

  return <video ref={videoRef} {...videoProps} />;
}

export default MultipleOutputs;
