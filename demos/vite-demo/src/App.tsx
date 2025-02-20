import './App.css'
import { setWasmBundleUrl } from '@swmansion/smelter-web-wasm'
import CanvasPage from './pages/CanvasPage'

setWasmBundleUrl("/assets/smelter.wasm");

function App() {
  return (
    <>
      <CanvasPage />
    </>
  )
}

export default App
