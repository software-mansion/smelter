import init, * as wasm from './generated/smelter/smelter';

/**
 * Loads and initializes wasm module required for the smelter to work.
 * @param wasmModuleUrl {string} - An URL for `smelter.wasm` file. The file is located in `dist` folder.
 */
export const loadWasmModule = (() => {
  let loadResult: Promise<wasm.InitOutput> | undefined = undefined;
  return async (wasmModuleUrl: string) => {
    if (loadResult) {
      await loadResult;
      return;
    }

    loadResult = init({ module_or_path: wasmModuleUrl });
    await loadResult;
  };
})();

export { wasm };
