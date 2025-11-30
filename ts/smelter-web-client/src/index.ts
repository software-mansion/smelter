import { SmelterOptions } from './manager';
import Smelter from './smelter/live';
import OfflineSmelter from './smelter/offline';

export * from './api';

export { OfflineSmelter, SmelterOptions };
export default Smelter;
