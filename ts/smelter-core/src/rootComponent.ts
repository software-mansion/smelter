import { _smelterInternals, useAfterTimestamp } from '@swmansion/smelter';
import { createElement, useEffect, type ReactElement } from 'react';

type SmelterOutputContext = _smelterInternals.SmelterOutputContext;
type ChildrenLifetimeContext = _smelterInternals.ChildrenLifetimeContext;

const globalDelayRef = Symbol();

export function OutputRootComponent({
  outputContext,
  outputRoot,
  childrenLifetimeContext,
}: {
  outputContext: SmelterOutputContext;
  outputRoot: ReactElement;
  childrenLifetimeContext: ChildrenLifetimeContext;
}) {
  useMinimalStreamDuration(childrenLifetimeContext);
  return createElement(
    _smelterInternals.SmelterContext.Provider,
    { value: outputContext },
    createElement(
      _smelterInternals.ChildrenLifetimeContextType.Provider,
      { value: childrenLifetimeContext },
      outputRoot
    )
  );
}

/**
 * Add minimal 1 second lifetime in case there are not live
 * components inside the scene.
 */
function useMinimalStreamDuration(childrenLifetimeContext: ChildrenLifetimeContext) {
  useEffect(() => {
    childrenLifetimeContext.removeRef(globalDelayRef);
    return () => {
      childrenLifetimeContext.removeRef(globalDelayRef);
    };
  }, []);
  const afterTimestamp = useAfterTimestamp(1000);
  console.log({ afterTimestamp });
  useEffect(() => {
    if (afterTimestamp) {
      childrenLifetimeContext.removeRef(globalDelayRef);
    }
  }, [afterTimestamp]);
}
