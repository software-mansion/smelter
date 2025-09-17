import path from 'node:path';
import fs from 'fs-extra';

class Mp4SuggestionMonitor {
  public mp4Files: string[];

  constructor() {
    const mp4sDir = path.resolve(process.cwd(), 'mp4s');
    let files: string[] = [];
    try {
      files = fs.readdirSync(mp4sDir);
    } catch {
      files = [];
    }
    this.mp4Files = files.filter(f => f.toLowerCase().endsWith('.mp4'));
  }
}
const mp4SuggestionsMonitor = new Mp4SuggestionMonitor();
export default mp4SuggestionsMonitor;
