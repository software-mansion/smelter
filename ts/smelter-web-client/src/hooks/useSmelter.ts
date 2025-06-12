import { useEffect, useState } from 'react';
import Smelter from '../smelter/live';
import type { SmelterInstanceOptions } from '../manager';

export function useSmelter(options: SmelterInstanceOptions): Smelter | undefined {
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

// export function useSmelter(options: SmelterInstanceOptions): Smelter | undefined {
//   const [smelter, setSmelter] = useState<Smelter>();
//   const [cleanupPromise, setCleanupPromise] = useState<Promise<void>>();
//   const [instanceOptions, setInstanceOptions] = useState<SmelterInstanceOptions>();
//   const count = useRef(0);
//
//   useEffect(() => {
//     void (async () => {
//       await cleanupPromise;
//       setInstanceOptions(options);
//     })();
//
//   }, [options, cleanupPromise]);
//
//   useEffect(() => {
//     if (!instanceOptions) {
//       return;
//     }
//
//     const smelter = new Smelter(options);
//     count.current++;
//     let c = count.current;
//
//     // TODO(noituri): Restart smelter instance
//     let cancel = false;
//     const promise = (async () => {
//       console.log('init', c);
//       await smelter.init();
//       await smelter.start();
//       if (!cancel) {
//         setSmelter(smelter);
//       }
//     })();
//
//     return () => {
//       cancel = true;
//       setCleanupPromise((prevCleanup) => (async () => {
//         await prevCleanup;
//         await promise.catch(() => { });
//         await smelter.terminate();
//         console.log("cleanup", c);
//       })());
//     }
//   }, [instanceOptions,]);
//
//   return smelter;
// }
