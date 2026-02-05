"use client"

import WhepClientVideo from "@/components/WhepClientVideo";
import { WhipClient } from "@/utils/whip-client";
import { useRef, useState } from "react";

// Base url of a WHIP/WHEP server. By default, Smelter exposes this server on
// port 9000, but the value can be changed via SMELTER_WHIP_WHEP_SERVER_PORT
// environment variable.
const SMELTER_WHIP_WHEP_URL = new URL("http://127.0.0.1:9000")

const WHIP_AUTH_TOKEN = "example_token"

// API of Node.js server from `/server` directory.
const BACKEND_URL = new URL("http://127.0.0.1:3001")

export default function Home() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-zinc-800 font-sans">
      <main className="flex flex-row min-h-screen justify-between py-32 px-16">
        <WhepClientVideo
          url={new URL("/whep/output", SMELTER_WHIP_WHEP_URL).toString()}
          poster="https://placehold.co/1920x1080/000000/333333?text=Waiting+for+stream..."
          playsInline autoPlay controls
          className='min-w-0  min-h-0 w-full h-full object-cover bg-black'
        />
        <Controls />
      </main>
    </div>
  );
}

function Controls() {
  const [showInstructions, setShowInstruction] = useState(true);

  const toggleInstructions = async () => {
    setShowInstruction(!showInstructions)
    await fetch(new URL("/layout-update", BACKEND_URL), {
      method: 'POST',
      mode: 'cors',
      headers: {
        'content-type': 'application/json',
      },
      body: JSON.stringify({ showInstructions: !showInstructions }),
    });
  }

  const startStream = async () => {
    await fetch(new URL("/start-rtmp-stream", BACKEND_URL), {
      method: 'POST',
      mode: 'cors',
      headers: {
        'content-type': 'application/json',
      },
      body: JSON.stringify({ url: 'rtmp://x.rtmp.youtube.com/live2/<YOUR STREAM KEY>' }),
    });
  }

  return (
    <div className="w-1/3 p-10 flex flex-col items-start">
      <button className="bg-purple-800 hover:bg-purple-700 text-white font-bold py-2 px-4 rounded mb-10 w-full" onClick={startStream}>
        Start stream
      </button>
      <Checkbox description="Show instructions" isChecked={showInstructions} onChange={toggleInstructions} />
    </div>
  )
}

function Checkbox(props: { description: string, isChecked: boolean, onChange: (update: boolean) => void }) {
  return (
    <div className="flex items-start gap-3 p-4 border border-slate-200 rounded-lg transition-colors cursor-pointer mb-10 w-full"
      onClick={() => props.onChange(!props.isChecked)}>

      <div className="flex items-center h-5">
        <input
          type="checkbox"
          checked={props.isChecked}
          onChange={() => props.onChange(!props.isChecked)}
          className="w-4 h-4 text-purple-800 border-gray-300 rounded focus:ring-purple-700 cursor-pointer"
        />
      </div>

      <div className="flex flex-col">
        <label
          className="font-medium text-slate-900 cursor-pointer text-white"
        >
          {props.description}
        </label>
      </div>
    </div>
  );
};
