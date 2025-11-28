import Smelter from '@swmansion/smelter-web-wasm';
import { useEffect, useState } from 'react';

export function useSmelter(): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  useEffect(() => {
    const smelter = new Smelter();

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
  }, []);
  return smelter;
}
