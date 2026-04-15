import PageHeader from "@/components/PageHeader";
import WhepClientVideo from "@/components/WhepClientVideo";

// Base url of a WHIP/WHEP server. By default, Smelter exposes this server on
// port 9000, but the value can be changed via SMELTER_WHIP_WHEP_SERVER_PORT
// environment variable.
const SMELTER_WHIP_WHEP_URL = new URL("http://127.0.0.1:9000");

export default function ViewerPage() {
  return (
    <div className="min-h-screen bg-background text-foreground font-sans">
      <PageHeader
        title="Viewer"
        statusDot="bg-accent animate-pulse"
        statusText="Watching"
      />

      <main className="max-w-6xl mx-auto px-6 py-8">
        <div className="bg-card border border-border rounded-lg overflow-hidden">
          <div className="aspect-video bg-black">
            <WhepClientVideo
              url={new URL("/whep/output", SMELTER_WHIP_WHEP_URL).toString()}
              poster="https://placehold.co/1920x1080/0f0f0f/27272a?text=Waiting..."
              playsInline autoPlay controls
              className="w-full h-full object-contain"
            />
          </div>
        </div>
        <p className="mt-6 text-sm text-muted text-center">
          Output stream from the Smelter server
        </p>
      </main>
    </div>
  );
}
