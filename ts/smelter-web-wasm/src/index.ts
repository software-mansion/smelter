import Smelter, { setWasmBundleUrl, SmelterOptions } from './compositor/compositor';

export { RegisterOutput, RegisterInput } from './compositor/api';
export { useSmelter } from './hooks/useSmelter';
export { setWasmBundleUrl, SmelterOptions };

export default Smelter;
