import React, { useCallback, useEffect, useState } from 'react';
import type Smelter from '@swmansion/smelter-web-client';
import { connectToWhepServer } from './connectToWhepServer';

type VideoProps = React.DetailedHTMLProps<
  React.VideoHTMLAttributes<HTMLVideoElement>,
  HTMLVideoElement
>;

type SmelterWhepOutputProps = {
  smelter: Smelter;
  children: React.ReactElement;
} & VideoProps;

const WHEP_SERVER_BASE_URL = "http://127.0.0.1:9000";

/**
 * Component responsible for registering new WHEP output that renders ReactElement passed as 
 * a child. It renders `<video>` tag that displays stream from that WHEP server.
 */
export default function SmelterWhepOutput(props: SmelterWhepOutputProps) {
  const { children, smelter, ...videoProps } = props;
  const [videoElement, setVideoElement] = useState<HTMLVideoElement | null>(null);

  const videoRef = useCallback(async (updatedVideo: HTMLVideoElement | null) => {
    setVideoElement(updatedVideo);
  }, []);

  useEffect(
    () => {
      if (!videoElement) {
        return;
      }

      const outputId = getNewOutputId();
      const promise = (async () => {
        const output = await smelter.registerOutput(outputId, children, {
          type: 'whep_server',
          video: {
            encoder: {
              type: "ffmpeg_h264",
              preset: "ultrafast"
            },
            resolution: {
              width: 1920,
              height: 1080,
            }
          },
          audio: {
            encoder: {
              type: "opus"
            }
          },
        });

        const stream = await connectToWhepServer(`${WHEP_SERVER_BASE_URL}${output.endpointRoute}`);
        // eslint-disable-next-line
        (videoElement as any).srcObject = stream;
        await videoElement.play()
      })();

      return () => {
        void promise
          .catch(console.error)
          .then(() => smelter.unregisterOutput(outputId))
          .catch(console.error);
      };
    },
    // eslint-disable-next-line
    [smelter, videoElement]
  );

  return <video ref={videoRef} {...videoProps} />;
}

const getNewOutputId = (() => {
  let counter = 1;
  return () => {
    const outputId = `output-${counter}`;
    counter += 1;
    return outputId;
  };
})();
