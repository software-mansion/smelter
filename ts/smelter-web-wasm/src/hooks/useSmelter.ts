import { useEffect, useState } from 'react';
import type { SmelterOptions } from '../compositor/compositor';
import Smelter from '../compositor/compositor';

export function useSmelter(options?: SmelterOptions): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  useEffect(() => {
    const smelter = new Smelter(options);

    let cancel = false;
    const promise = (async () => {
      await smelter.init();
      await smelter.start();
      if (!cancel) {
        setSmelter(smelter);
      }
    })();

    return () => {
      cancel = true;
      void (async () => {
        await promise.catch(() => {});
        await smelter.terminate();
      })();
    };
  }, [options?.framerate, (options?.framerate as any)?.num, (options?.framerate as any)?.den]);
  return smelter;
}
