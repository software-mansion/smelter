"use client"

import WhepClientVideo from "@/components/WhepClientVideo";

export default function Home() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-zinc-800 font-sans">
      <main className="flex flex-row min-h-screen w-full flex-col items-center justify-between py-32 px-16 sm:items-start">
        <WhepClientVideo
          url="http://127.0.0.1:9000/whep/output"
          poster="https://placehold.co/1280x720/000000/333333?text=Waiting+for+stream..."
          playsInline autoPlay controls
          className='min-w-0  min-h-0 w-full h-full object-cover bg-black'
        />
        <Controls />
      </main>
    </div>
  );
}

function Controls() {
  return (
    <div className="w-1/3 p-10 flex flex-col items-start">
      <button className="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded mb-10 w-full">
        Start screen share
      </button>
      <button className="bg-blue-500 hover:bg-blue-700 text-white font-bold py-2 px-4 rounded mb-10 w-full">
        Start camera
      </button>
      <Checkbox description="Show instructions" isChecked onChange={() => console.log("onchange")} />
    </div>
  )
}

function Checkbox(props: { description: string, isChecked: boolean, onChange: (update: boolean) => void }) {
  return (
    <div className="flex items-start gap-3 p-4 border border-slate-200 rounded-lg hover:bg-slate-50 transition-colors cursor-pointer mb-10 w-full"
      onClick={() => props.onChange(!props.isChecked)}>

      <div className="flex items-center h-5">
        <input
          type="checkbox"
          checked={props.isChecked}
          onChange={() => props.onChange(!props.isChecked)}
          className="w-4 h-4 text-blue-600 border-gray-300 rounded focus:ring-blue-500 cursor-pointer"
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
