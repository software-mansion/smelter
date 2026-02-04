import WhepClientVideo from "@/components/WhepClientVideo";

// Base url of a WHIP/WHEP server. By default, Smelter exposes this server on
// port 9000, but the value can be changed via SMELTER_WHIP_WHEP_SERVER_PORT
// environment variable.
const SMELTER_WHIP_WHEP_URL = new URL("http://127.0.0.1:9000")

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
      </main>
    </div>
  );
}
