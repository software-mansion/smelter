import React, { useCallback, useEffect, useState } from 'react';
import type Smelter from '@swmansion/smelter-web-wasm';
import { getNewOutputId } from './util';

type VideoProps = React.DetailedHTMLProps<
  React.VideoHTMLAttributes<HTMLVideoElement>,
  HTMLVideoElement
>;

type SmelterVideoProps = {
  smelter: Smelter;
  audio?: boolean;
  children: React.ReactElement;
} & VideoProps;

export default function SmelterVideoOutput(props: SmelterVideoProps) {
  const { children, smelter, audio, ...videoProps } = props;
  const [videoElement, setVideoElement] = useState<HTMLVideoElement | null>(null);

  const videoRef = useCallback(async (updatedVideo: HTMLVideoElement | null) => {
    setVideoElement(updatedVideo);
  }, []);

  useEffect(() => {
    if (!videoElement) {
      return;
    }

    const outputId = getNewOutputId();
    const promise = (async () => {
      const { stream } = await smelter.registerOutput(outputId, children, {
        type: 'stream',
        video: {
          resolution: {
            width: Number(videoProps.width ?? videoElement?.width),
            height: Number(videoProps.height ?? videoElement?.height),
          },
        },
        audio,
      });
      if (stream) {
        videoElement.srcObject = stream;
        await videoElement.play();
      }
    })();

    return () => {
      void promise
        .catch(console.error)
        .then(() => smelter.unregisterOutput(outputId))
        .catch(console.error);
    };
  }, [videoProps.width, videoProps.height, smelter, audio, videoElement]);

  return <video ref={videoRef} {...videoProps} />;
}
