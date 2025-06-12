import { useEffect, useRef, useState } from 'react';
import type { SmelterOptions } from '../compositor/compositor';
import Smelter from '../compositor/compositor';

export function useSmelter(options?: SmelterOptions): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  const count = useRef(1);
  useEffect(() => {
    const smelter = new Smelter(options);

    let id = count.current;
    count.current++;
    let cancel = false;
    (async () => {
      console.log('Init', id);
      await smelter.init();
      await smelter.start();
      if (!cancel) {
        setSmelter(smelter);
      }
    })();

    return () => {
      console.log('Cancel', id);
      cancel = true;
      void smelter.terminate();
    };
  }, [options?.framerate, (options?.framerate as any)?.num, (options?.framerate as any)?.den]);
  return smelter;
}
