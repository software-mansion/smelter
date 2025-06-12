import { useEffect, useState } from 'react';
import Smelter from '../smelter/live';
import type { SmelterInstanceOptions } from '../manager';

export function useSmelter(options: SmelterInstanceOptions): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  useEffect(() => {
    const smelter = new Smelter(options);

    let cancel = false;
    (async () => {
      await smelter.init();
      await smelter.start();
      if (!cancel) {
        setSmelter(smelter);
      }
    })();

    return () => {
      cancel = true;
      void smelter.terminate()
    };
  }, [options?.url]);

  return smelter;
}
