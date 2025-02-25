import React, { useCallback, useEffect, useState } from 'react';
import Smelter from '@swmansion/smelter-web-wasm';

type VideoProps = React.DetailedHTMLProps<
  React.VideoHTMLAttributes<HTMLVideoElement>,
  HTMLVideoElement
>;

type CompositorVideoProps = {
  onVideoCreate?: (smelter: Smelter) => Promise<void>;
  onVideoStarted?: (smelter: Smelter) => Promise<void>;
  children: React.ReactElement;
} & VideoProps;

export default function CompositorVideo(props: CompositorVideoProps) {
  const { onVideoCreate, onVideoStarted, children, ...videoProps } = props;
  const [smelter, setSmelter] = useState<Smelter | undefined>(undefined);

  const videoRef = useCallback(
    async (video: HTMLVideoElement | null) => {
      if (!video) {
        return;
      }

      const smelter = new Smelter({});

      await smelter.init();

      if (onVideoCreate) {
        await onVideoCreate(smelter);
      }

      const { stream } = await smelter.registerOutput('output', children, {
        type: 'stream',
        video: {
          resolution: {
            width: Number(videoProps.width ?? video.width),
            height: Number(videoProps.height ?? video.height),
          },
        },
        audio: true,
      });

      await smelter.start();
      setSmelter(smelter);

      if (onVideoStarted) {
        await onVideoStarted(smelter);
      }
      if (stream) {
        video.srcObject = stream;
        await video.play();
      }
    },
    [onVideoCreate, onVideoStarted, videoProps.width, videoProps.height, children]
  );

  useEffect(() => {
    return () => {
      if (smelter) {
        void smelter.terminate();
      }
    };
  }, [smelter]);

  return <video ref={videoRef} {...videoProps} />;
}
