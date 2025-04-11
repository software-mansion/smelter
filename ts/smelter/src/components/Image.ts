import { createElement, useContext, useEffect, useState } from 'react';
import type { ComponentBaseProps, SceneComponent } from '../component.js';
import { createSmelterComponent } from '../component.js';
import { View, type Api } from '../index.js';
import { imageRefIntoRawId } from '../types/refs/imageRef.js';
import { newInternalImageId } from '../context/internalImageIdManager.js';
import { newBlockingTask } from '../hooks.js';
import { SmelterContext } from '../context/index.js';
import { isValidImageType } from '../types/utils.js';
import type { RegisterImage } from '../types/resource.js';

export type ImageProps = Omit<ComponentBaseProps, 'children'> &
  (
    | {
        imageId: Api.RendererId;
        source?: never; // Ensuring 'source' cannot be used alongside 'imageId'
      }
    | {
        source: string;
        imageId?: never; // Ensuring 'imageId' cannot be used alongside 'source'
      }
  );

type ImageSceneBuliderProps = Omit<ImageProps, 'imageId'> & { imageId: string };

export const InnerImage = createSmelterComponent<ImageSceneBuliderProps>(sceneBuilder);

function Image(props: ImageProps) {
  const ctx = useContext(SmelterContext);
  const [internalImageId, setInternalImageId] = useState(0);
  const [isImageRegistered, setIsImageRegistered] = useState(!!props.imageId);

  if ((props.imageId && props.source) || (!props.imageId && !props.source)) {
    throw new Error('Either "imageId" or "source" must be provided, but not both.');
  }

  useEffect(() => {
    if (props.imageId) {
      setIsImageRegistered(true);
      return;
    }
    setIsImageRegistered(false);

    const newImageId = newInternalImageId();
    const pathOrUrl: Pick<RegisterImage, 'serverPath' | 'url'> =
      props.source?.startsWith('http://') || props.source?.startsWith('https://')
        ? { url: props.source }
        : { serverPath: props.source };
    const extension = props.source?.split('.').pop();
    const assetType = extension && isValidImageType(extension) ? extension : undefined;

    let registerPromise: Promise<any>;

    if (!assetType) {
      throw new Error('Unsupported image type');
    }

    const task = newBlockingTask(ctx);
    setInternalImageId(newImageId);
    void (async () => {
      try {
        registerPromise = ctx.registerImage(newImageId, {
          ...pathOrUrl,
          assetType,
        });
        await registerPromise;
        setIsImageRegistered(true);
      } finally {
        task.done();
      }
    })();

    return () => {
      task.done();
      void (async () => {
        await registerPromise.catch(() => {});
        await ctx.unregisterImage(newImageId);
      })();
    };
  }, [props.source, props.imageId]);

  if (!isImageRegistered) {
    return createElement(View, {});
  } else if (props.source) {
    return createElement(InnerImage, {
      ...props,
      imageId: imageRefIntoRawId({
        type: 'output-specific-image',
        id: internalImageId,
        outputId: ctx.outputId,
      }),
    });
  } else if (props.imageId) {
    return createElement(InnerImage, {
      ...props,
      imageId: imageRefIntoRawId({ type: 'global', id: props.imageId }),
    });
  }

  return createElement(View, {});
}

function sceneBuilder(props: ImageSceneBuliderProps, _children: SceneComponent[]): Api.Component {
  return {
    type: 'image',
    id: props.id,
    image_id: props.imageId,
  };
}

export default Image;
