import React, { useCallback, useEffect, useState } from 'react';
import type Smelter from '@swmansion/smelter-web-wasm';
import { getNewOutputId } from './util';

type VideoProps = React.DetailedHTMLProps<
  React.VideoHTMLAttributes<HTMLVideoElement>,
  HTMLVideoElement
>;

type SmelterWhipProps = {
  smelter: Smelter;
  endpointUrl: string;
  bearerToken?: string;
  audio?: boolean;
  video: {
    resolution: { width: number; height: number };
    maxBitrate?: number;
  };
  children: React.ReactElement;
} & VideoProps;

export default function SmelterWhipOutput(props: SmelterWhipProps) {
  const { children, smelter, audio, video, endpointUrl, bearerToken, ...videoProps } = props;
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
        type: 'whip',
        endpointUrl,
        bearerToken,
        video: {
          resolution: video.resolution,
          maxBitrate: video.maxBitrate,
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
  }, [
    smelter,
    audio,
    video?.resolution?.width,
    video?.resolution?.height,
    video?.maxBitrate,
    videoElement,
    bearerToken,
    endpointUrl,
  ]);

  return <video ref={videoRef} {...videoProps} />;
}
