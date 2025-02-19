import React, { useCallback, useEffect, useState } from 'react';
import { Smelter } from '@swmansion/smelter-web-wasm';

type CanvasProps = React.DetailedHTMLProps<
  React.CanvasHTMLAttributes<HTMLCanvasElement>,
  HTMLCanvasElement
>;

type CompositorCanvasProps = {
  onCanvasCreate?: (smelter: Smelter) => Promise<void>;
  onCanvasStarted?: (smelter: Smelter) => Promise<void>;
  children: React.ReactElement;
} & CanvasProps;

export default function CompositorCanvas(props: CompositorCanvasProps) {
  const { onCanvasCreate, onCanvasStarted, children, ...canvasProps } = props;
  const [smelter, setSmelter] = useState<Smelter | undefined>(undefined);

  const canvasRef = useCallback(
    async (canvas: HTMLCanvasElement | null) => {
      if (!canvas) {
        return;
      }

      const smelter = new Smelter({});

      await smelter.init();

      if (onCanvasCreate) {
        await onCanvasCreate(smelter);
      }

      await smelter.registerOutput('output', children, {
        type: 'canvas',
        video: {
          canvas: canvas,
          resolution: {
            width: Number(canvasProps.width ?? canvas.width),
            height: Number(canvasProps.height ?? canvas.height),
          },
        },
        audio: true,
      });

      await smelter.start();
      setSmelter(smelter);

      if (onCanvasStarted) {
        await onCanvasStarted(smelter);
      }
    },
    [onCanvasCreate, onCanvasStarted, canvasProps.width, canvasProps.height, children]
  );

  useEffect(() => {
    return () => {
      if (smelter) {
        void smelter.terminate();
      }
    };
  }, [smelter]);

  return <canvas ref={canvasRef} {...canvasProps}></canvas>;
}
