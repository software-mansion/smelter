import { useEffect, useState } from 'react';
import Smelter from '../smelter/live';
import type { SmelterInstanceOptions } from '../manager';

export function useSmelter2(options: SmelterInstanceOptions): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  const [_, setPromiseQueue] = useState<Promise<void>>(Promise.resolve());

  useEffect(() => {
    const smelter = new Smelter(options);

    // TODO(noituri): Restart smelter instance
    let cancel = false;
    setPromiseQueue((promise) => promise.finally(async () => {
      await smelter.init();
      await smelter.start();
      if (!cancel) {
        setSmelter(smelter);
      }
    }));

    return () => {
      cancel = true;
      setPromiseQueue((promise) => promise.finally(async () => {
        await smelter.terminate();
      }));
    }
  }, [options]);

  return smelter;
}

export function useSmelter(options: SmelterInstanceOptions): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  const [cleanupPromise, setCleanupPromise] = useState<Promise<void>>();
  const [instanceOptions, setInstanceOptions] = useState<SmelterInstanceOptions>();

  useEffect(() => {
    void (async () => {
      await cleanupPromise;
      setInstanceOptions(options);
    })();

  }, [options, cleanupPromise]);

  useEffect(() => {
    if (!instanceOptions) {
      return;
    }

    const smelter = new Smelter(options);

    // TODO(noituri): Restart smelter instance
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
      setCleanupPromise((prevCleanup) => (async () => {
        await prevCleanup;
        await promise.catch(() => { });
        await smelter.terminate();
      })());
    }
  }, [instanceOptions]);

  return smelter;
}
