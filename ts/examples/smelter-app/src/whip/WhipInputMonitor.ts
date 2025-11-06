export class WhipInputMonitor {
  private username: string;
  private isStreamLive: boolean = true;
  private onUpdateFn?: () => void;
  private lastAckTimestamp = Date.now();

  private constructor(username: string) {
    this.username = username;
  }

  public static async startMonitor(username: string): Promise<WhipInputMonitor> {
    return new WhipInputMonitor(username);
  }
  public getLastAckTimestamp(): number {
    return this.lastAckTimestamp;
  }

  public isLive(): boolean {
    return this.isStreamLive;
  }

  public touch(): void {
    this.lastAckTimestamp = Date.now();
    console.log(`[whip] Touch ${this.username}`);
  }
}
