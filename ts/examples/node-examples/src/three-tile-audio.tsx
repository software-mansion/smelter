import Smelter from '@swmansion/smelter-node';
import { InputStream, Rescaler, View } from '@swmansion/smelter';
import path from 'path';
import { useState, useEffect } from 'react';
import {
  ffplayStartRtpServerAsync,
  // ffplayStartRtmpServerAsync,
  ffmpegSendVideoFromMp4,
} from './utils';

const VIDEO_RESULOTION = {
  width: 1920,
  height: 1080,
};

type Resolution = {
  width: number;
  height: number;
};

type Position = {
  top?: number;
  bottom?: number;
  left?: number;
  right?: number;
};

type SceneProps = {
  inputId: string;
  resolution: Resolution;
  position: Position;
};

function Scene({ inputId, resolution, position }: SceneProps) {
  return (
    <View
      id="scaled"
      style={{
        ...resolution,
        ...position,
      }}
      transition={{
        durationMs: 2500,
      }}>
      <Rescaler
        style={{
          rescaleMode: 'fit',
        }}>
        <InputStream inputId={inputId} />
      </Rescaler>
    </View>
  );
}

type SceneValue = '1' | '1to2' | '2' | '2to3' | '3' | '3to1';

function ExampleApp() {
  const [currentSceneState, setCurrentSceneState] = useState<SceneValue>('1');
  const [transitionalScene, setTransitionalScene] = useState(false);

  useEffect(() => {
    let timeoutNormal = null,
      timeoutTransitional = null;
    if (!transitionalScene) {
      timeoutNormal = setTimeout(() => {
        switch (currentSceneState) {
          case '1':
            setCurrentSceneState('1to2');
            setTransitionalScene(true);
            console.log('Changed to transitional scene 1to2!');
            break;
          case '2':
            setCurrentSceneState('2to3');
            setTransitionalScene(true);
            console.log('Changed to transitional scene 2to3!');
            break;
          case '3':
            setCurrentSceneState('3to1');
            setTransitionalScene(true);
            console.log('Changed to transitional scene 3to1!');
            break;
          default:
            console.log('It should not happen!');
        }
      }, 10000);
    } else {
      timeoutTransitional = setTimeout(() => {
        switch (currentSceneState) {
          case '1to2':
            setCurrentSceneState('2');
            setTransitionalScene(false);
            console.log('Changed to normal scene 2!');
            break;
          case '2to3':
            setCurrentSceneState('3');
            setTransitionalScene(false);
            console.log('Changed to normal scene 3!');
            break;
          case '3to1':
            setCurrentSceneState('1');
            setTransitionalScene(false);
            console.log('Changed to normal scene 1!');
            break;
          default:
            console.log('It should not happen!');
        }
      }, 2500);
    }
    return () => {
      timeoutNormal && clearTimeout(timeoutNormal);
      timeoutTransitional && clearTimeout(timeoutTransitional);
    };
  }, [transitionalScene]);

  const currentScene =
    currentSceneState === '1' ? (
      <Scene inputId="input1_video" resolution={VIDEO_RESULOTION} position={{ top: 0, left: 0 }} />
    ) : currentSceneState === '2' ? (
      <Scene inputId="input2_video" resolution={VIDEO_RESULOTION} position={{ top: 0, right: 0 }} />
    ) : currentSceneState === '3' ? (
      <Scene
        inputId="input3_video"
        resolution={VIDEO_RESULOTION}
        position={{ bottom: 0, left: 0 }}
      />
    ) : currentSceneState === '1to2' ? (
      <Scene
        inputId="input1_video"
        resolution={{ width: 1, height: 1 }}
        position={{ top: 0, left: 0 }}
      />
    ) : currentSceneState === '2to3' ? (
      <Scene
        inputId="input2_video"
        resolution={{ width: 1, height: 1 }}
        position={{ top: 0, right: 0 }}
      />
    ) : (
      <Scene
        inputId="input3_video"
        resolution={{ width: 1, height: 1 }}
        position={{ bottom: 0, left: 0 }}
      />
    );

  return (
    <View
      style={{
        backgroundColor: '#000000FF',
        width: VIDEO_RESULOTION.width,
        height: VIDEO_RESULOTION.height,
      }}>
      {currentScene}
    </View>
  );
}

async function run() {
  const smelter = new Smelter();
  await smelter.init();

  // await ffplayStartRtmpServerAsync(9012);
  //
  // await smelter.registerOutput('output1', <ExampleApp />, {
  //   type: 'rtmp_client',
  //   url: 'rtmp://127.0.0.1:9012',
  //   video: {
  //     encoder: {
  //       type: 'ffmpeg_h264',
  //       preset: 'ultrafast',
  //     },
  //     resolution: {
  //       width: VIDEO_RESULOTION.width,
  //       height: VIDEO_RESULOTION.height,
  //     },
  //   },
  // });

  await ffplayStartRtpServerAsync('127.0.0.1', 9012);

  await smelter.registerOutput('output1', <ExampleApp />, {
    type: 'rtp_stream',
    ip: '127.0.0.1',
    port: 9012,
    video: {
      resolution: {
        width: VIDEO_RESULOTION.width,
        height: VIDEO_RESULOTION.height,
      },
      encoder: {
        type: 'ffmpeg_h264',
        preset: 'ultrafast',
      },
    },
  });

  await smelter.registerInput('input1_video', {
    type: 'rtp_stream',
    port: 8222,
    video: {
      decoder: 'ffmpeg_h264',
    },
  });

  await smelter.registerInput('input2_video', {
    type: 'rtp_stream',
    port: 8224,
    video: {
      decoder: 'ffmpeg_h264',
    },
  });

  await smelter.registerInput('input3_video', {
    type: 'rtp_stream',
    port: 8226,
    video: {
      decoder: 'ffmpeg_h264',
    },
  });

  ffmpegSendVideoFromMp4(8222, path.join(__dirname, '../.assets/lachrymaQuiet.mp4'));
  ffmpegSendVideoFromMp4(8224, path.join(__dirname, '../.assets/peacefieldQuiet.mp4'));
  ffmpegSendVideoFromMp4(8226, path.join(__dirname, '../.assets/satanizedQuiet.mp4'));

  await smelter.start();
}
void run();
