import React, { useCallback, useEffect, useRef, useState } from 'react';
import type Smelter from '@swmansion/smelter-web-wasm';

type CanvasProps = React.DetailedHTMLProps<
  React.CanvasHTMLAttributes<HTMLCanvasElement>,
  HTMLCanvasElement
>;

type SmelterCanvasProps = {
  smelter: Smelter;
  audio?: boolean;
  children: React.ReactElement;
} & CanvasProps;

type UpdatedProps = {
  smelter: Smelter;
  width?: number | string;
  height?: number | string;
  audio?: boolean;
};

type RegisterOptions = UpdatedProps & {
  outputId: string;
  canvas: HTMLCanvasElement;
};

const getNewOutputId = (() => {
  let counter = 1;
  return () => {
    const outputId = `output-${counter}`;
    counter += 1;
    return outputId;
  };
})();

export default function SmelterCanvasOutput(props: SmelterCanvasProps) {
  const { children, smelter, audio, ...canvasProps } = props;
  const [updatedProps, setUpdatedProps] = useState<UpdatedProps | undefined>();
  const [registerOptions, setRegisterOptions] = useState<RegisterOptions | undefined>(undefined);
  const key = useRef<number>(1);

  const canvasRef = useCallback(
    async (updatedCanvas: HTMLCanvasElement | null) => {
      if (!updatedCanvas || !updatedProps || updatedCanvas === registerOptions?.canvas) {
        return;
      }
      const outputId = getNewOutputId();
      setRegisterOptions({
        ...updatedProps,
        outputId,
        canvas: updatedCanvas,
      });
    },
    [updatedProps, registerOptions]
  );

  useEffect(() => {
    // force new canvas
    key.current += 1;

    setUpdatedProps({
      smelter,
      width: canvasProps.width,
      height: canvasProps.height,
      audio,
    });
  }, [canvasProps.width, canvasProps.height, smelter, audio]);

  useEffect(() => {
    if (!registerOptions) {
      return;
    }
    const { audio, outputId, width, height, canvas } = registerOptions;
    const promise = smelter.registerOutput(outputId, children, {
      type: 'canvas',
      video: {
        canvas,
        resolution: {
          width: Number(width ?? canvas?.width),
          height: Number(height ?? canvas?.height),
        },
      },
      audio,
    });

    return () => {
      void promise
        .catch(console.error)
        .then(() => smelter.unregisterOutput(outputId))
        .catch(console.error);
    };
  }, [registerOptions]);

  return <canvas key={key.current} ref={canvasRef} {...canvasProps} />;
}
