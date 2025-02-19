import { loadWasmModule, Renderer } from '@swmansion/smelter-browser-render';
import { useEffect, useState } from 'react';
import NotoSansFont from '../../assets/NotoSans.ttf';

export function useRenderer(): Renderer | null {
  const [renderer, setRenderer] = useState<Renderer | null>(null);
  useEffect(() => {
    const setupRenderer = async () => {
      await loadWasmModule('/assets/smelter.wasm');
      const renderer = await Renderer.create({
        streamFallbackTimeoutMs: 500,
      });

      await renderer.registerImage('img', {
        asset_type: 'gif',
        url: 'https://media.tenor.com/eFPFHSN4rJ8AAAAM/example.gif',
      });
      await renderer.registerFont(NotoSansFont);

      setRenderer(renderer);
    };

    setupRenderer().catch(err => console.error(err));
  }, []);

  return renderer;
}
