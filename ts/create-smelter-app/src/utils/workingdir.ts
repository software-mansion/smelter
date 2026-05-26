import fs from 'fs-extra';
import { confirmPrompt } from './prompts';

export async function ensureProjectDir(directory: string) {
  const alreadyExists = await fs.pathExists(directory);
  if (alreadyExists) {
    const stat = await fs.stat(directory);
    // remove cwd unless it's an empty directory
    if (!stat.isDirectory || (await fs.readdir(directory)).length > 0) {
      if (await confirmPrompt(`Path "${directory}" already exists, Do you want to override it?`)) {
        console.log(`Removing ${directory}.`);
        await fs.remove(directory);
      } else {
        console.error('Aboring ...');
        process.exit(1);
      }
    }
  }
  await fs.mkdirp(directory);
}
