import fs from 'fs/promises';
import { confirmPrompt } from './prompts';

export async function ensureProjectDir(directory: string) {
  const alreadyExists = await pathExists(directory);
  if (alreadyExists) {
    const stat = await fs.stat(directory);
    // remove cwd unless it's an empty directory
    if (!stat.isDirectory() || (await fs.readdir(directory)).length > 0) {
      if (await confirmPrompt(`Path "${directory}" already exists, Do you want to override it?`)) {
        console.log(`Removing ${directory}.`);
        await fs.rm(directory, { recursive: true, force: true });
      } else {
        console.error('Aborting ...');
        process.exit(1);
      }
    }
  }
  await fs.mkdir(directory, { recursive: true });
}

async function pathExists(filePath: string): Promise<boolean> {
  try {
    await fs.access(filePath);
    return true;
  } catch {
    return false;
  }
}
