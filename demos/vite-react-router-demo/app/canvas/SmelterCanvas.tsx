import Smelter from "@swmansion/smelter-web-wasm";
import { useCallback, useEffect, useState, type ReactElement } from "react";

type CanvasProps = React.DetailedHTMLProps<
  React.CanvasHTMLAttributes<HTMLCanvasElement>,
  HTMLCanvasElement
>;

type SmelterCanvasProps = CanvasProps & {
  onSmelterCreated?: (smelter: Smelter) => Promise<void> | void;
  onSmelterStarted?: (smelter: Smelter) => Promise<void> | void;
  children: ReactElement,
};

type SmelterState = { smelter: Smelter, initPromise: Promise<void> }

/** 
  * Helper component that starts smelter with single output to HTML Canvas.
  */
export default function SmelterCanvas(props: SmelterCanvasProps) {
  const { children, onSmelterCreated, onSmelterStarted, ...canvasProps } = props;

  const [smelterState, setSmelterState] = useState<SmelterState | undefined>();

  const canvasRef = useCallback((canvasElement: HTMLCanvasElement | null) => {
    if (!canvasElement) {
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

      await smelter.registerOutput('output', children, {
        type: 'canvas',
        video: {
          canvas: canvasElement,
          resolution: { width: Number(props.width ?? 1920), height: Number(props.height ?? 1080) },
        },
        audio: true,
      });

      await smelter.start()
      if (onSmelterStarted) {
        await onSmelterStarted(smelter)
      }
    })();
  }, [onSmelterStarted, onSmelterCreated, props.width, props.height])

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
    <canvas ref={canvasRef} {...canvasProps} />
  )
}
