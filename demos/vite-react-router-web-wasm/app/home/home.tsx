
export function Home() {
  return (
    <main className="flex flex-col items-center justify-center">
      <div className="flex-1 flex flex-col items-center gap-16 pt-16 pb-16">
        <div className="w-[500px] max-w-[100vw] p-4">
          <img src="/smelter-logo.svg"
            className="block w-full"
          />
        </div>
      </div>
      <div className="flex flex-col w-[1200px] max-w-9/10">
        {resources.map(({ href, text, title }) => (
          <a key={href} href={href}>
            <div className="rounded-3xl w-full border border-gray-200 p-6 m-4 space-y-4">
              <h3 className="text-xl text-gray-200">{title}</h3>
              <p className="text-gray-200">{text}</p>
            </div>
          </a>
        ))}
      </div>
    </main>
  );
}

const resources = [
  {
    href: "/stream",
    title: "WHIP stream ➡️",
    text: "Simple setup to stream camera and screen over WHIP",
  },
  {
    href: "/canvas",
    title: "Output to HTML Canvas ➡️",
    text: "Simple setup to stream camera and screen over WHIP",
  },
];
