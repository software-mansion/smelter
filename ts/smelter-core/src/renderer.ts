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
type Container = Renderer;
type HostContext = object;
type Instance = HostComponent;
type TextInstance = string;
type ChildSet = Array<string | Instance>;
type Timeout = ReturnType<typeof setTimeout>;

const HostConfig: Reconciler.HostConfig<
  Type,
  Props,
  Container,
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

  getRootHostContext(_rootContainer: Container) {
    return null;
  },

  getChildHostContext(
    parentHostContext: HostContext,
    _type: Type,
    _rootContainer: Container
  ): HostContext {
    return parentHostContext;
  },

  prepareForCommit(_containerInfo: Container): Record<string, any> | null {
    return null;
  },

  resetAfterCommit(container: Container): void {
    container.onUpdate();
  },

  createInstance(
    type: Type,
    props: Props,
    _rootContainer: Container,
    _hostContext: HostContext,
    _internalHandle: any
  ): HostComponent {
    if (type === 'smelter') {
      return new HostComponent(props.props, props.sceneBuilder);
    } else {
      throw new Error(`Unknown type ${type}`);
    }
  },

  appendInitialChild(parentInstance: Instance, child: Instance | TextInstance): void {
    parentInstance.children.push(child);
  },

  finalizeInitialChildren(
    _instance: Instance,
    _type: Type,
    _props: Props,
    _rootContainer: Container,
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
    _rootContainer: Container,
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
    _rootContainer: Container,
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

  preparePortalMount(_container: Container) {
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

  createContainerChildSet(_container: Container): ChildSet {
    return [];
  },
  appendChildToContainerChildSet(childSet: ChildSet, child: Instance | TextInstance) {
    childSet.push(child);
  },
  finalizeContainerChildren(_container: Container, _newChildren: (Instance | TextInstance)[]) {},
  replaceContainerChildren(_container: Container, _newChildren: (Instance | TextInstance)[]) {},

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
};

const CompositorRenderer = Reconciler(HostConfig);

type RendererOptions = {
  rootElement: React.ReactElement;
  onUpdate: () => void;
  idPrefix: string;
  logger: Logger;
};

// docs
interface FiberRootNode {
  tag: number; // 0
  containerInfo: Renderer;
  pendingChildren: HostComponent[];
  current: any;
}

class Renderer {
  public readonly root: FiberRootNode;
  public readonly onUpdate: () => void;
  private logger: Logger;
  private lastScene?: Api.Component;

  constructor({ rootElement, onUpdate, idPrefix, logger }: RendererOptions) {
    this.logger = logger;
    this.onUpdate = onUpdate;

    this.root = CompositorRenderer.createContainer(
      this, // container tag
      LegacyRoot,
      null, // hydrationCallbacks
      false, // isStrictMode
      null, // concurrentUpdatesByDefaultOverride
      idPrefix, // identifierPrefix
      logger.error, // onRecoverableError
      null // transitionCallbacks
    );

    CompositorRenderer.updateContainer(rootElement, this.root, null, () => {});
  }

  public scene(): Api.Component {
    if (this.lastScene) {
      // Renderer was already stopped just return old scene
      return this.lastScene;
    }

    // When resetAfterCommit is called `this.root.current` is not updated yet, so we need to rely
    // on `pendingChildren`. I'm not sure it is always populated, so there is a fallback to
    // `root.current`.

    const rootComponent =
      this.root.pendingChildren[0] ?? rootHostComponent(this.root.current, this.logger);
    return rootComponent.scene();
  }

  public stop() {
    this.lastScene = this.scene();
    CompositorRenderer.updateContainer(null, this.root, null, () => {});
  }
}

function rootHostComponent(root: any, logger: Logger): HostComponent {
  logger.error('No pendingChildren found, this might be an error.');
  let current = root;
  while (current) {
    if (current?.stateNode instanceof HostComponent) {
      return current?.stateNode;
    }
    current = current.child;
  }
  throw new Error('No smelter host component found in the tree.');
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
