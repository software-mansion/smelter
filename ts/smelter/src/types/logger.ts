export interface Logger {
  error(...params: any[]): void;
  warn(...params: any[]): void;
  info(...params: any[]): void;
  debug(...params: any[]): void;
  trace(...params: any[]): void;
  child(bindings: Record<string, any>): Logger;
}
