import { useCallback, useEffect, useState } from 'react';
import Smelter from '../smelter/live';
import type { SmelterInstanceOptions } from '../manager';

export function useSmelter(options: SmelterInstanceOptions): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  const [promiseQueue, setPromiseQueue] = useState<(() => Promise<void>)[]>([]);
  const [isWaiting, setIsWaiting] = useState(false);

  const enqueue = useCallback((fn: () => Promise<void>) => {
    setPromiseQueue(queue => [
      ...queue,
      fn,
    ])
  }, []);

  useEffect(() => {
    const smelter = new Smelter(options);

    let cancel = false;
    enqueue(async () => {
      await smelter.init();
      await smelter.start();
      if (!cancel) {
        setSmelter(smelter);
      }
    });

    return () => {
      cancel = true;
      enqueue(async () => await smelter.terminate());
    };
  }, [options?.url, enqueue]);

  useEffect(() => {
    if (isWaiting) {
      return;
    }
    setIsWaiting(true);

    const fn = promiseQueue[0];
    if (fn) {
      void (async () => {
        await fn();
        setPromiseQueue(promiseQueue.slice(1));
        setIsWaiting(false);
      })();
    }
  }, [promiseQueue, isWaiting]);

  return smelter;
}
