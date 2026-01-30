import WhepClientVideo from "@/components/WhepClientVideo";

export default function Home() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-zinc-50 font-sans dark:bg-black">
      <main className="flex min-h-screen w-full max-w-3xl flex-col items-center justify-between py-32 px-16 bg-white dark:bg-black sm:items-start">
        <div className="flex flex-col items-center gap-6 text-center sm:items-start sm:text-left">
          <WhepClientVideo
            url="http://127.0.0.1:9000/whep/output"
            poster="https://placehold.co/1280x720/000000/333333?text=Waiting+for+stream..."
            playsInline autoPlay controls
          />
        </div>
      </main>
    </div>
  );
}
