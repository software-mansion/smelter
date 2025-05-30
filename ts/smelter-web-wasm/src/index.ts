import Smelter, { setWasmBundleUrl, SmelterOptions } from './compositor/compositor';
import SmelterWhipOutput from './helpers/components/SmelterWhipOutput'
import SmelterVideoOutput from './helpers/components/SmelterVideoOutput'
import SmelterCanvasOutput from './helpers/components/SmelterCanvasOutput'

export { RegisterOutput, RegisterInput } from './compositor/api';
export { useSmelter } from './helpers/hooks/useSmelter'
export { setWasmBundleUrl, SmelterWhipOutput, SmelterVideoOutput, SmelterCanvasOutput, SmelterOptions };

export default Smelter;
