import View, { ViewProps } from './components/View.js';
import Image, { ImageProps } from './components/Image.js';
import Text, { TextProps } from './components/Text.js';
import InputStream, { InputStreamProps } from './components/InputStream.js';
import Rescaler, { RescalerProps } from './components/Rescaler.js';
import WebView, { WebViewProps } from './components/WebView.js';
import Shader, { ShaderParam, ShaderParamStructField, ShaderProps } from './components/Shader.js';
import Tiles, { TilesProps } from './components/Tiles.js';
import { EasingFunction, Transition } from './components/common.js';
import {
  useAudioInput,
  useInputStreams,
  useAfterTimestamp,
  useBlockingTask,
  useCurrentTimestamp,
} from './hooks.js';
import Show, { ShowProps } from './components/Show.js';
import { SlideShow, Slide, SlideProps, SlideShowProps } from './components/SlideShow.js';
import Mp4, { Mp4Props } from './components/Mp4.js';

export {
  RegisterRtpInput,
  RegisterMp4Input,
  RegisterHlsInput,
  RegisterWhipServerInput,
  RegisterWhepClientInput,
  RegisterRtmpServerInput,
  RegisterV4l2Input,
} from './types/input.js';
export {
  RegisterRtpOutput,
  RegisterMp4Output,
  RegisterHlsOutput,
  RegisterWhipClientOutput,
  RegisterWhepServerOutput,
  RegisterRtmpClientOutput,
} from './types/output.js';

export * as Inputs from './types/input.js';
export * as Outputs from './types/output.js';
export * as Renderers from './types/resource.js';
export * as Api from './api.js';
export * as _smelterInternals from './internal.js';

export {
  View,
  ViewProps,
  Image,
  ImageProps,
  Text,
  TextProps,
  InputStream,
  InputStreamProps,
  Rescaler,
  RescalerProps,
  WebView,
  WebViewProps,
  Shader,
  ShaderProps,
  Tiles,
  TilesProps,
  Show,
  ShowProps,
  Slide,
  SlideProps,
  SlideShow,
  SlideShowProps,
  Mp4,
  Mp4Props,
};

export { useInputStreams, useAudioInput, useBlockingTask, useAfterTimestamp, useCurrentTimestamp };

export { ShaderParam, ShaderParamStructField, EasingFunction, Transition };
