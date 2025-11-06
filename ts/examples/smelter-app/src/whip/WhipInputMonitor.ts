export class WhipInputMonitor {
    private channelId: string;
    private isStreamLive: boolean = true;
    private shouldStop = false;
    private onUpdateFn?: () => void;
    private lastAckTimestamp = Date.now();
    private idleThresholdMs = 20_000;
  
    private constructor(channelId: string) {
      this.channelId = channelId;
      void this.monitor();
    }
  
    public static async startMonitor(channelId: string): Promise<WhipInputMonitor> {
      if (!channelId) {
        throw new Error(`Unable to find live streams for ${channelId}`);
      }
      return new WhipInputMonitor(channelId);
    }
    public getLastAckTimestamp(): number {
      return this.lastAckTimestamp;
    }
  
    public stop() {
      this.shouldStop = true;
    }
  
    public isLive(): boolean {
      return this.isStreamLive;
    }
  
    public touch(): void {
      this.lastAckTimestamp = Date.now();
      console.log(`[whip] Touch ${this.channelId}`);
      if (!this.isStreamLive) {
        this.isStreamLive = true;
        this.onUpdateFn?.();
      }
    }

    public onUpdate(onUpdateFn: () => void): void {
      this.onUpdateFn = onUpdateFn;
      onUpdateFn();
    }
  
    private async monitor() {
      while (!this.shouldStop) {
        const now = Date.now();
        const shouldBeLive = now - this.lastAckTimestamp < this.idleThresholdMs;
        if (shouldBeLive !== this.isStreamLive) {
          this.isStreamLive = shouldBeLive;
          this.onUpdateFn?.();
        }
        await new Promise(resolve => setTimeout(resolve, 2_000));
      }
    }
  }