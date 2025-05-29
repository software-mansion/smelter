import { useEffect, useState } from 'react';
import Smelter, { SmelterOptions } from '../../compositor/compositor';

export function useSmelter(options?: SmelterOptions): Smelter | undefined {
  // TODO(noituri): Handle props change
  // - All outputs and inputs need to be recreated
  const [smelter, setSmelter] = useState<Smelter>();
  useEffect(() => {
    const prevState = smelter && smelter.getResources();
    const smelterInstance = new Smelter(options);

    let cancel = false;
    const promise = (async () => {
      await smelterInstance.init();
      await smelterInstance.start();
      if (prevState) {
        for (const [id, req] of Object.entries(prevState.inputs)) {
          await smelterInstance.registerInput(id, req);
        }
        for (const [id, [req, root]] of Object.entries(prevState.outputs)) {
          await smelterInstance.registerOutput(id, root, req);
        }
        for (const [id, req] of Object.entries(prevState.images)) {
          await smelterInstance.registerImage(id, req);
        }
        for (const [id, req] of Object.entries(prevState.shaders)) {
          await smelterInstance.registerShader(id, req);
        }
        for (const url of prevState.fontUrls) {
          await smelterInstance.registerFont(url);
        }
      }
      if (!cancel) {
        setSmelter(smelterInstance);
      }
    })();

    return () => {
      cancel = true;
      void (async () => {
        await promise.catch(() => { });
        await smelterInstance.terminate();
      })();
    };
  }, [options]);
  return smelter;
}
