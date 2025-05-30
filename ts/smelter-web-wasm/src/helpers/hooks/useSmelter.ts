import { useEffect, useState } from 'react';
import Smelter, { SmelterOptions } from '../../compositor/compositor';

export function useSmelter(options?: SmelterOptions): Smelter | undefined {
  const [smelter, setSmelter] = useState<Smelter>();
  useEffect(() => {
    const prevState = smelter && smelter.getResources();
    const smelterInstance = new Smelter(options);

    console.log("update", prevState);

    let cancel = false;
    console.log("smelter preinit", cancel);
    const promise = (async () => {
      console.log("smelter init", cancel);
      if (smelter) {
        try {
          console.log("smelter terminate", cancel);
          await smelter.terminate();
          console.log("smelter terminated", cancel);
        } catch (e) {
          console.error("Error terminating smelter", e);
        }
      }
      console.log("smelter init call", cancel);
      await smelterInstance.init();
      console.log("smelter start call", cancel);
      await smelterInstance.start();
      if (prevState) {
        console.log("smelter restore state", cancel);
        for (const [id, req] of Object.entries(prevState.inputs)) {
          await smelterInstance.registerInput(id, req);
        }
        // for (const [id, [req, root]] of Object.entries(prevState.outputs)) {
        //   await smelterInstance.registerOutput(id, root, req);
        // }
        for (const [id, req] of Object.entries(prevState.images)) {
          await smelterInstance.registerImage(id, req);
        }
        for (const [id, req] of Object.entries(prevState.shaders)) {
          await smelterInstance.registerShader(id, req);
        }
        for (const url of prevState.fontUrls) {
          await smelterInstance.registerFont(url);
        }

        console.log("smelter state restored", cancel);
      }

      console.log("smelter init done", cancel);
      if (!cancel) {
        console.log("smelter updated");
        setSmelter(smelterInstance);
      }
    })();

    return () => {
      console.log("smelter cancel");
      cancel = true;
      void (async () => {
        await promise.catch(() => { });
        await smelterInstance.terminate();
      })();
    };
  }, [options]);
  return smelter;
}
