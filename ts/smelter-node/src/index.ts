import type { SmelterManager } from '@swmansion/smelter-core';
import Smelter from './live/compositor';
import ExistingInstanceManager, { ExistingInstanceOptions } from './manager/existingInstance';
import LocallySpawnedInstanceManager, {
  LocallySpawnedInstanceOptions,
} from './manager/locallySpawnedInstance';
import OfflineSmelter from './offline/compositor';

export * from './api';

export default Smelter;
export {
  OfflineSmelter,
  LocallySpawnedInstanceManager,
  LocallySpawnedInstanceOptions,
  ExistingInstanceManager,
  ExistingInstanceOptions,
  SmelterManager,
};
