import Smelter from "@swmansion/smelter-web-wasm";
import { useCallback, useEffect, useState, type ReactElement, type ReactNode } from "react";

type VideoProps = React.DetailedHTMLProps<
  React.VideoHTMLAttributes<HTMLVideoElement>,
  HTMLVideoElement
>;

type WhipStreamProps = VideoProps & {
  endpointUrl: string
  bearerToken?: string
  onSmelterCreated?: (smelter: Smelter) => Promise<void> | void;
  onSmelterStarted?: (smelter: Smelter) => Promise<void> | void;
  children: ReactElement,
};

type SmelterState = { smelter: Smelter, initPromise: Promise<void> }

/** 
  * Helper component that starts smelter with single WHIP output.
  * Preview of the stream is displayed in a `<video />` tag.
  */
export default function WhipStream(props: WhipStreamProps) {
  const { endpointUrl, bearerToken, children, onSmelterCreated, onSmelterStarted, ...videoProps } = props;

  const [smelterState, setSmelterState] = useState<SmelterState | undefined>();

  const videoRef = useCallback((videoElement: HTMLVideoElement | null) => {
    if (!videoElement) {
      return
    }
    const smelter = new Smelter();
    const initPromise = smelter.init()
    setSmelterState({
      smelter,
      initPromise,
    });

    (async () => {
      await initPromise;
      if (onSmelterCreated) {
        await onSmelterCreated(smelter)
      }

      const { stream } = await smelter.registerOutput('output',
        children,
        {
          type: 'whip',
          endpointUrl,
          bearerToken,
          video: {
            resolution: { width: 1920, height: 1080 },
          },
          audio: true,
        }
      );
      if (!stream) {
        throw new Error('Missing stream from register output.')
      }

      await smelter.start()
      if (onSmelterStarted) {
        await onSmelterStarted(smelter)
      }

      videoElement.srcObject = stream;
      await videoElement.play();
    })();
  }, [endpointUrl, bearerToken, onSmelterStarted, onSmelterCreated])

  useEffect(() => {
    return () => {
      if (smelterState) {
        smelterState.initPromise
          .catch(() => { })
          .then(() => smelterState.smelter.terminate())
      }
    };
  }, [smelterState]);

  return (
    <video ref={videoRef} {...videoProps} />
  )
}
