import { createContext, useContext, useEffect, useState } from 'react';
import { useAfterTimestamp } from '../hooks.js';

export class ChildrenLifetimeContext {
  private childrenRefs: Set<Symbol> = new Set();
  private onChange: () => void;

  constructor(onChange: () => void) {
    this.onChange = onChange;
  }

  public addRef(ref: Symbol) {
    this.childrenRefs.add(ref);
    this.onChange();
  }

  public removeRef(ref: Symbol) {
    this.childrenRefs.delete(ref);
    this.onChange();
  }

  public isDone(): boolean {
    return this.childrenRefs.size === 0;
  }
}

/**
 * Context that exposes API to children to register themself as playing/in-progress. Some components
 * will change their behavior based on the state of its in-direct children, e.g. Slides component will
 * not switch Slide until children are finished.
 */
export const ChildrenLifetimeContextType = createContext(new ChildrenLifetimeContext(() => {}));

/**
 * Internal helper hook that can be used inside other components to propagate
 * their duration/lifetime to the parents.
 */
export function useTimeLimitedComponent(timestamp: number) {
  const childrenLifetimeContext = useContext(ChildrenLifetimeContextType);
  const afterTimestamp = useAfterTimestamp(timestamp);
  const [ref, setComponentRef] = useState<Symbol>();
  useEffect(() => {
    let ref = Symbol();
    setComponentRef(ref);
    childrenLifetimeContext.addRef(ref);
    return () => {
      childrenLifetimeContext.removeRef(ref);
    };
  }, [timestamp]);

  useEffect(() => {
    if (ref && afterTimestamp) {
      childrenLifetimeContext.removeRef(ref);
    }
  }, [afterTimestamp, ref]);
}

export function useCompletableComponent(completed: boolean) {
  const childrenLifetimeContext = useContext(ChildrenLifetimeContextType);
  console.log({ childrenLifetimeContext });
  const [ref, setComponentRef] = useState<Symbol>();
  useEffect(() => {
    let ref = Symbol();
    setComponentRef(ref);
    childrenLifetimeContext.addRef(ref);
    return () => {
      childrenLifetimeContext.removeRef(ref);
    };
  }, []);

  useEffect(() => {
    if (ref && completed) {
      childrenLifetimeContext.removeRef(ref);
    }
  }, [completed, ref]);
}
