import { useEffect, useRef, useState } from 'react';
import Smelter from '../smelter/live';
import type { SmelterInstanceOptions } from '../manager';

export function useSmelter(options: SmelterInstanceOptions): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  const prevSmelter = useRef<Smelter>();

  useEffect(() => {
    const smelter = new Smelter(options);
    const smelterToTerminate = prevSmelter.current;
    prevSmelter.current = smelter;

    let cancel = false;
    (async () => {
      if (smelterToTerminate) {
        console.log("Terminate");
      }
      await smelterToTerminate?.terminate().catch(() => { });
      console.log("Init");
      await smelter.init();
      await smelter.start();
      if (!cancel) {
        setSmelter(smelter);
      }
    })();

    return () => {
      cancel = true;
    };
  }, [options?.url]);

  useEffect(() => {
    return () => {
      if (prevSmelter.current) {
        console.log("Terminate unmount");
      }
      // It runs only during unmount
      void prevSmelter.current?.terminate().catch(() => { });
    }
  }, [])

  return smelter;
}
