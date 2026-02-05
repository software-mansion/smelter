import Link from "next/link";

export default function Home() {
  return (
    <div className="page">
      <main className="max-w-2xl mx-auto px-6 py-24">
        <header className="mb-12">
          <h1 className="text-3xl font-semibold tracking-tight mb-2">Smelter Demo</h1>
          <p className="text-muted">WebRTC streaming with real-time video processing</p>
        </header>

        <section className="mb-12">
          <h2 className="section-title">About</h2>
          <div className="card p-6">
            <p className="text-foreground/90 mb-4">This demo application showcases Smelter&apos;s capabilities:</p>
            <ul className="space-y-2 text-foreground/80">
              <li className="flex gap-3"><span className="text-accent">•</span>Stream camera or screen share to Smelter over WHIP</li>
              <li className="flex gap-3"><span className="text-accent">•</span>Apply effects and overlay elements in real-time</li>
              <li className="flex gap-3"><span className="text-accent">•</span>Broadcast resulting stream over WHEP</li>
            </ul>
          </div>
        </section>

        <section>
          <h2 className="section-title">Get Started</h2>
          <div className="grid grid-cols-2 gap-4">
            <Link href="/viewer" className="card p-6 hover:border-accent/50 transition-all group">
              <div className="text-lg font-medium mb-1 group-hover:text-accent transition-colors">Viewer</div>
              <p className="text-sm text-muted">Watch the stream</p>
            </Link>
            <Link href="/streamer" className="bg-accent hover:bg-accent-hover rounded-lg p-6 transition-all">
              <div className="text-lg font-medium mb-1">Streamer</div>
              <p className="text-sm text-white/70">Start broadcasting</p>
            </Link>
          </div>
        </section>
      </main>
    </div>
  );
}
