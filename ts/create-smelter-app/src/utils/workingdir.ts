import { mkdirp, pathExists, readdir, remove, stat } from 'fs-extra';
import { confirmPrompt } from './prompts';

export async function ensureProjectDir(directory: string) {
  const alreadyExists = await pathExists(directory);
  if (alreadyExists) {
    const dirStat = await stat(directory);
    // remove cwd unless it's an empty directory
    if (!dirStat.isDirectory || (await readdir(directory)).length > 0) {
      if (await confirmPrompt(`Path "${directory}" already exists, Do you want to override it?`)) {
        console.log(`Removing ${directory}.`);
        await remove(directory);
      } else {
        console.error('Aboring ...');
        process.exit(1);
      }
    }
  }
  await mkdirp(directory);
}
