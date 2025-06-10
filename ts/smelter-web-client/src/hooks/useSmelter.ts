import { useEffect, useRef, useState } from 'react';
import Smelter from '../smelter/live';
import type { SmelterInstanceOptions } from '../manager';

export function useSmelter(options: SmelterInstanceOptions): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  const cleanUpPromise = useRef<Promise<void>>();

  useEffect(() => {
    const smelter = new Smelter(options);

    let cancel = false;
    // TODO(noituri): Add smelter.restart()
    const promise = (async () => {
      await cleanUpPromise.current;

      await smelter.init();
      await smelter.start();
      if (!cancel) {
        setSmelter(smelter);
      }
    })();

    return () => {
      cancel = true;
      cleanUpPromise.current = (async () => {
        await promise.catch(() => {});
        await smelter.terminate();
      })();
    };
  }, [options]);

  return smelter;
}
