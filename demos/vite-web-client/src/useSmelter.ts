import Smelter from '@swmansion/smelter-web-client';
import { useEffect, useState } from 'react';

export function useSmelter(url: string): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  useEffect(() => {
    const smelter = new Smelter({ url });

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
        await promise.catch(() => { });
        await smelter.terminate();
      })();
    };
  }, [url]);
  return smelter;
}
