import React, { useCallback, useState } from 'react';
import Smelter from '@swmansion/smelter-web-wasm';

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

export default function CompositorVideo(props: CompositorVideoProps) {
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
