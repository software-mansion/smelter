import './App.css'
import { setWasmBundleUrl } from '@swmansion/smelter-web-wasm'
import CanvasPage from './pages/CanvasPage'
import { useState } from 'react';

setWasmBundleUrl("/assets/smelter.wasm");

function App() {
  const [showExample, setShowExample] = useState(false);

  return (
    showExample
      ? <CanvasPage />
      : (
        <div>
          <button onClick={() => setShowExample(true)} style={{ margin: 10 }}>
            Launch example
          </button>
        </div>
      )

  )
}

export default App
