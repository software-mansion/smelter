import Link from "next/link";

export default function Home() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-zinc-800 font-sans">
      <main className="flex flex-col gap-10 min-h-screen w-full max-w-3xl flex-col items-stretch justify-start py-32 px-16">
        <div>
          <p className="text-white">This is a simple demo application that:</p>
          <ul className="text-white list-disc list-inside">
            <li>Streams camera or screen share to the Smelter instance over WHIP.</li>
            <li>Apply effects, adds overlay elements, ...</li>
            <li>Broadcasts resulting stream over WHEP.</li>
          </ul>
        </div>

        <div className="flex flex-row gap-10">
          <Link href="/viewer" className="flex-1 bg-purple-800 hover:bg-purple-700 text-white font-bold py-2 px-4 rounded mb-10">
            Open as a viewer
          </Link>
          <Link href="/streamer" className="flex-1 bg-purple-800 hover:bg-purple-700 text-white font-bold py-2 px-4 rounded mb-10">
            Open as a streamer
          </Link>
        </div>
      </main>
    </div>
  );
}
