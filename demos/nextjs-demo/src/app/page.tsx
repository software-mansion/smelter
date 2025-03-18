'use client';

import './page.css'
import CanvasPage from './CanvasPage'
import { useState } from 'react';

function Home() {
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

export default Home
