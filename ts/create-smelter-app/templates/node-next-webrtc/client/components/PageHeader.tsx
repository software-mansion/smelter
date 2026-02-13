import Link from "next/link";

type PageHeaderProps = {
  title: string;
  statusDot?: string;
  statusText?: string;
};

export default function PageHeader({ title, statusDot, statusText }: PageHeaderProps) {
  return (
    <header className="border-b border-border py-4">
      <div className="max-w-6xl mx-auto px-6 flex items-center justify-between">
        <div className="flex items-center gap-4">
          <Link href="/" className="text-muted hover:text-foreground transition-colors">← Back</Link>
          <span className="text-border">|</span>
          <h1 className="font-medium">{title}</h1>
        </div>
        {statusDot && statusText && (
          <div className="flex items-center gap-2">
            <span className={`w-2 h-2 rounded-full ${statusDot}`} />
            <span className="text-sm text-muted">{statusText}</span>
          </div>
        )}
      </div>
    </header>
  );
}
