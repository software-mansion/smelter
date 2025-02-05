import type { SmelterManager } from '@swmansion/smelter-core';
import Smelter from './live/compositor';
import ExistingInstanceManager from './manager/existingInstance';
import LocallySpawnedInstanceManager from './manager/locallySpawnedInstance';
import OfflineSmelter from './offline/compositor';

export default Smelter;
export { OfflineSmelter, LocallySpawnedInstanceManager, ExistingInstanceManager, SmelterManager };
