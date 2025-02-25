import { useState } from 'react';
import './App.css';
import Counter from './examples/Counter';
import InputMp4Example from './examples/InputMp4Example';
import ComponentMp4Example from './examples/ComponentMp4Example';
import MultipleInstances from './examples/MultipleInstances';
import Camera from './examples/CameraExample';
import ScreenCapture from './examples/ScreenCaptureExample';
import { setWasmBundleUrl } from '@swmansion/smelter-web-wasm';
import WhipExample from './examples/WhipExample';
import DemoExample from './examples/Demo';
import MultipleOutputs from './examples/MultipleOutputs';
import MediaStreamInput from './examples/MediaStreamExample';

setWasmBundleUrl('/assets/smelter.wasm');

function App() {
  const EXAMPLES = {
    counter: <Counter />,
    inputMp4: <InputMp4Example />,
    componentMp4: <ComponentMp4Example />,
    whip: <WhipExample />,
    multipleCompositors: <MultipleInstances />,
    multipleOutputs: <MultipleOutputs />,
    camera: <Camera />,
    screenCapture: <ScreenCapture />,
    mediaStream: <MediaStreamInput />,
    home: <Home />,
    demo: <DemoExample />,
  };
  const [currentExample, setCurrentExample] = useState<keyof typeof EXAMPLES>('home');

  return (
    <>
      <h1>Examples</h1>
      <div className="examples-tabs">
        <button onClick={() => setCurrentExample('home')}>Home</button>

        <button onClick={() => setCurrentExample('demo')}>Demo</button>

        <h3>Smelter examples</h3>
        <button onClick={() => setCurrentExample('whip')}>WHIP</button>
        <button onClick={() => setCurrentExample('inputMp4')}>Input Stream MP4</button>
        <button onClick={() => setCurrentExample('componentMp4')}>Component MP4</button>
        <button onClick={() => setCurrentExample('multipleCompositors')}>
          Multiple Smelter instances
        </button>
        <button onClick={() => setCurrentExample('multipleOutputs')}>Multiple outputs</button>
        <button onClick={() => setCurrentExample('camera')}>Camera</button>
        <button onClick={() => setCurrentExample('screenCapture')}>Screen Capture</button>
        <button onClick={() => setCurrentExample('mediaStream')}>MediaStream</button>

        <h3>Smelter rendering engine examples</h3>
        <button onClick={() => setCurrentExample('counter')}>Counter</button>
      </div>
      <div className="card">{EXAMPLES[currentExample]}</div>
    </>
  );
}

function Home() {
  return (
    <div style={{ textAlign: 'left' }}>
      <h2>Packages:</h2>
      <h3>
        <code>@swmansion/smelter-web-wasm</code> - Smelter in the browser
      </h3>
      <li>
        <code>Demo</code> - Demo that combine most of the below features in one example. Stream a
        scene that includes a camera, screen share and mp4 file to Twitch. Add{' '}
        <code>?twitchKey=mytwitchstreamkey</code> query param with your Twitch stream key to stream
        it yourself.
      </li>
      <br />
      <li>
        <code>WHIP</code> - Streams Mp4 file to Twitch. Add{' '}
        <code>?twitchKey=mytwitchstreamkey</code> query param with your Twitch stream key to stream
        it yourself.
      </li>
      <li>
        <code>Input Stream Mp4</code> - Register MP4 file as an input stream and render output on
        canvas.
      </li>
      <li>
        <code>Component Mp4</code> - Add 2 MP4 component (one after the other) to the scene and
        render output on canvas.
      </li>
      <li>
        <code>Multiple Smelter instances</code> - Runs multiple Smelter instances at the same time.
      </li>
      <li>
        <code>Multiple outputs</code> - Runs single smelter instance with multiple outputs.
      </li>
      <li>
        <code>Camera</code> - Use webcam as an input and render output on canvas.
      </li>
      <li>
        <code>Screen Capture</code> - Use screen capture as an input and render output on canvas.
      </li>
      <li>
        <code>MediaStream</code> - Pass MediaStream object as an input. In this example it will be
        camera.
      </li>
      <h3>
        <code>@swmansion/smelter-browser-render</code> - Rendering engine from Smelter
      </h3>
      <li>
        <code>Counter</code> - Render a GIF + counter trigged by user(with a button).
      </li>
    </div>
  );
}

export default App;
