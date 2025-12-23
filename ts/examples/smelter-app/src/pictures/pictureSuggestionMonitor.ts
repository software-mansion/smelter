import path from 'node:path';
import fs from 'fs-extra';

class PictureSuggestionMonitor {
  public pictureFiles: string[];

  constructor() {
    const picturesDir = path.resolve(process.cwd(), 'pictures');
    let files: string[] = [];
    try {
      files = fs.readdirSync(picturesDir);
    } catch {
      files = [];
    }
    const exts = ['.jpg', '.jpeg', '.png', '.gif', '.svg'];
    this.pictureFiles = files.filter(f => exts.some(ext => f.toLowerCase().endsWith(ext)));
  }
}
const pictureSuggestionsMonitor = new PictureSuggestionMonitor();
export default pictureSuggestionsMonitor;
