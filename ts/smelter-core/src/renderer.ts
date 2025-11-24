// eslint-disable-next-line import/no-named-as-default
import Reconciler from 'react-reconciler';
import { DefaultEventPriority, LegacyRoot } from 'react-reconciler/constants';
import type { Api } from './api';
import type { _smelterInternals } from '@swmansion/smelter';
import type React from 'react';
import type { Logger } from 'pino';

type SceneBuilder<P> = _smelterInternals.SceneBuilder<P>;
type SceneComponent = _smelterInternals.SceneComponent;

export class HostComponent {
  public props: object;
  public sceneBuilder: SceneBuilder<object>;
  public children: (Instance | TextInstance)[] = [];

  constructor(props: object, sceneBuilder: SceneBuilder<object>) {
    this.props = props;
    this.sceneBuilder = sceneBuilder;
  }

  public scene(): Api.Component {
    const children = this.children.map(child =>
      typeof child === 'string' ? child : child.scene()
    );
    return this.sceneBuilder(this.props, groupTextComponents(children));
  }
}

type Type = string;
type Props = {
  props: object;
  sceneBuilder: SceneBuilder<object>;
};
type RootContainer = Renderer;
type HostContext = object;
type Instance = HostComponent;
type TextInstance = string;
type ChildSet = Array<string | Instance>;
type Timeout = ReturnType<typeof setTimeout>;

const HostConfig: Reconciler.HostConfig<
  Type,
  Props,
  RootContainer,
  Instance,
  TextInstance,
  void, // SuspenseInstance
  void, // HydratableInstance
  Instance, //PublicInstance
  HostContext,
  object, // UpdatePayload
  ChildSet,
  Timeout, // TimeoutHandle
  -1 // NoTimeout
> = {
  getPublicInstance(instance: Instance | TextInstance) {
    return instance as Instance;
  },

  getRootHostContext(_rootContainer: RootContainer) {
    return null;
  },

  getChildHostContext(
    parentHostContext: HostContext,
    _type: Type,
    _rootContainer: RootContainer
  ): HostContext {
    return parentHostContext;
  },

  prepareForCommit(_containerInfo: RootContainer): Record<string, any> | null {
    return null;
  },

  resetAfterCommit(rootContainer: RootContainer): void {
    console.log('react onUpdate', rootContainer);
    rootContainer['onUpdate']();
  },

  createInstance(
    type: Type,
    props: Props,
    _rootContainer: RootContainer,
    _hostContext: HostContext,
    _internalHandle: any
  ): HostComponent {
    if (type === 'smelter') {
      return new HostComponent(props.props, props.sceneBuilder);
    } else {
      throw new Error(`Unknown type ${type}`);
    }
  },

  /*
   * This function will be called after `createInstance` or `cloneInstance` (with keepChildren == false)
   *
   * It adds components children. Tree in persistent mode is immutable, so this function can only be called
   * after component creation, but before component is part of the tree.
   */
  appendInitialChild(parentInstance: Instance, child: Instance | TextInstance): void {
    parentInstance.children.push(child);
  },

  finalizeInitialChildren(
    _instance: Instance,
    _type: Type,
    _props: Props,
    _rootContainer: RootContainer,
    _hostContext: HostContext
  ): boolean {
    // if true commitMount will be called
    return false;
  },

  prepareUpdate(
    _instance: Instance,
    _type: Type,
    _oldProps: Props,
    newProps: Props,
    _rootContainer: RootContainer,
    _hostContext: HostContext
  ): object | null {
    // TODO: optimize, it always triggers update
    return newProps;
  },

  shouldSetTextContent(_type: Type, _props: Props): boolean {
    return false;
  },
  createTextInstance(
    text: string,
    _rootContainer: RootContainer,
    _hostContext: HostContext,
    _internalHandle: any
  ) {
    return text;
  },

  scheduleTimeout: setTimeout,
  cancelTimeout: clearTimeout,
  noTimeout: -1,
  isPrimaryRenderer: true,
  warnsIfNotActing: true,
  supportsMutation: false,
  supportsPersistence: true,
  supportsHydration: false,

  getInstanceFromNode(_node: any) {
    throw new Error(`getInstanceFromNode not implemented`);
  },

  beforeActiveInstanceBlur() {},
  afterActiveInstanceBlur() {},

  preparePortalMount(_rootContainer: RootContainer) {
    throw new Error(`preparePortalMount not implemented`);
  },

  prepareScopeUpdate(_scopeInstance: any, _instance: any) {
    throw new Error(`prepareScopeUpdate not implemented`);
  },

  getInstanceFromScope(_scopeInstance) {
    throw new Error(`getInstanceFromScope not implemented`);
  },

  getCurrentEventPriority(): Reconciler.Lane {
    return DefaultEventPriority;
  },

  detachDeletedInstance(_node: Instance): void {},

  //
  // Persistence methods
  //

  /**
   * In this mode tree should be immutable so this is a primary method of updating elements.
   *
   * - cloneInstance is creating a new object based on previous instance
   * - if keepChildren is false new children will be added via appendInitialChild before tree is replaced
   */
  cloneInstance(
    instance: Instance,
    _updatePayload: object | null,
    _type: Type,
    _oldProps: Props,
    newProps: Props,
    _internalInstanceHandle: any,
    keepChildren: boolean,
    _recyclableInstance: Instance | null
  ) {
    const newInstance = new HostComponent(newProps.props, newProps.sceneBuilder);
    if (keepChildren) {
      newInstance.children = [...instance.children];
      return newInstance;
    } else {
      return newInstance;
    }
  },

  cloneHiddenInstance(
    _instance: Instance,
    _type: Type,
    props: Props,
    _internalInstanceHandle: any
  ): Instance {
    return new HostComponent(props.props, props.sceneBuilder);
  },

  cloneHiddenTextInstance(
    _instance: Instance,
    text: string,
    _internalInstanceHandle: any
  ): TextInstance {
    return text;
  },

  /*
   * Interaction with root container
   *
   * Container children represent root of the entire rendering tree.
   * For our use case there will always be only one root component
   *
   * - createContainerChildSet returns an empty structure that can hold root components
   * - appendChildToContainerChildSet adds new element to list of root components. In our case
   *   it will be called only once per ChildSet
   * - replaceContainerChildren should replace old tree with the new one. This function is
   *   called after finalizeContainerChildren. In this implementation we just take first element
   *   from the list and use it as a final render scene.
   */

  createContainerChildSet(_rootContainer: RootContainer): ChildSet {
    return [];
  },
  appendChildToContainerChildSet(childSet: ChildSet, child: Instance | TextInstance) {
    childSet.push(child);
  },
  finalizeContainerChildren(
    _rootContainer: RootContainer,
    _newChildren: (Instance | TextInstance)[]
  ) {},

  /*
   * Replace entire tree in the RootContainer
   */
  replaceContainerChildren(rootContainer: RootContainer, newChildren: ChildSet) {
    const newChild = newChildren[0];
    if (!newChild) {
      return;
    }
    if (newChild instanceof HostComponent) {
      rootContainer['rootComponent'] = newChild;
    } else {
      rootContainer.logger.warn('Expected HostComponent as root (text component received)');
    }
  },
};

const CompositorRenderer = Reconciler(HostConfig);

type RendererOptions = {
  rootElement: React.ReactElement;
  onUpdate: () => void;
  idPrefix: string;
  logger: Logger;
};

// TODO: docs
interface FiberRootNode {
  tag: number; // 0
  containerInfo: Renderer;
  pendingChildren: HostComponent[];
  current: any;
}

class Renderer {
  public readonly logger: Logger;
  private rootNode: FiberRootNode;

  // private, but accessible in HostConfig callbacks via renderer['onUpdate']
  private onUpdate: () => void;
  // private, but accessible in HostConfig callbacks via renderer['rootComponent']
  private rootComponent?: HostComponent;

  constructor({ rootElement, onUpdate, idPrefix, logger }: RendererOptions) {
    this.logger = logger;
    this.onUpdate = onUpdate;

    this.rootNode = CompositorRenderer.createContainer(
      this, // container tag
      LegacyRoot,
      null, // hydrationCallbacks
      false, // isStrictMode
      null, // concurrentUpdatesByDefaultOverride
      idPrefix, // identifierPrefix
      logger.error, // onRecoverableError
      null // transitionCallbacks
    );

    CompositorRenderer.updateContainer(rootElement, this.rootNode, null, () => {});
  }

  public scene(): Api.Component {
    return this.rootComponent ? this.rootComponent.scene() : { type: 'view' };
  }

  public stop() {
    CompositorRenderer.updateContainer(null, this.rootNode, null, () => {});
  }
}

function groupTextComponents(components: SceneComponent[]): SceneComponent[] {
  const groupedComponents: SceneComponent[] = [];
  let currentString: string | null = null;
  for (const component of components) {
    if (typeof component === 'string') {
      if (currentString === null) {
        currentString = component;
      } else {
        currentString = `${currentString}${component}`;
      }
    } else {
      if (currentString !== null) {
        groupedComponents.push(currentString);
        currentString = null;
      }
      groupedComponents.push(component);
    }
  }
  if (currentString !== null) {
    groupedComponents.push(currentString);
  }

  return groupedComponents;
}

export default Renderer;
